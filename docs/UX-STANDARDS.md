# Editor UX standards

Adopted 2026-11-17. This is the standard every editor surface is built and
reviewed against. It exists because the user named the editor's UI as the
product's weak point: surfaces feel unfinished, cluttered or overcomplicated.
Read it before touching any UI. The choices here are agent-selected defaults
that follow Unity and Unreal; the user can change any of them.

Confirmed user requirements this serves: a Unity or Unreal creator feels at
home immediately; everyday quality-of-life behavior and keyboard use are
non-negotiable; the look is clean, simple and readable; the Rot identity is
part of the appeal and can be switched off.

## 1. Principles

1. **Familiar first.** Use the name, place, shortcut and behavior that Unity
   and Unreal share. Where they differ, follow Unity and offer Unreal's as an
   option only if it is cheap. Never invent a new pattern for a solved problem.
2. **Content over chrome.** The viewport and the creator's data get the
   pixels. Every permanent control must earn its place.
3. **One obvious place.** Each action lives where the reference engines put
   it: object actions in the context menu, properties in the Inspector, view
   options in the viewport, settings in Preferences.
4. **Show state, do not explain it.** If a control needs a sentence beside it
   to be understood, redesign the control.
5. **Fast and quiet.** Instant response, no layout jumps, no repaint when
   nothing changes, no message unless something needs attention.
6. **One layout, two skins.** Rot and Plain share widgets, working layouts,
   control sizes and behavior. Use shared palette tokens, artwork and
   wording for their presentation differences.

## 2. Vocabulary

Use these words in the UI, the documentation and MCP tool descriptions. The
right-hand column is what the code and older documents call the same thing.

| Use | Unity / Unreal equivalent | Replaces |
| --- | --- | --- |
| Hierarchy | Hierarchy / Outliner | Objects |
| Inspector | Inspector / Details | Inspector |
| Assets | Project window / Content Browser | Content |
| Console | Console / Output Log | (none today) |
| Scene, Game (view tabs) | Scene and Game views / Viewport and PIE | Scene View, Player View |
| Object | GameObject / Actor | Object, Group, Light as separate kinds |
| Component | Component | fixed object fields |
| Script | Script / Blueprint class | Behavior, binding, source |
| Prefab | Prefab / Blueprint | (none today) |
| Play, Pause, Step, Stop | Play mode / Play In Editor | Reset, Runtime, Control runtime |
| Build, Build and Run | Build / Package | Export application |
| Preferences, Project Settings | Preferences, Project Settings | Workspace and shortcuts |
| AI connection | (none) | Rot connection |

Never show these internal words to a creator: revision, generation, authored,
accepted, admitted, guard, token, catalog, publication, closure, bounded,
binding, owner, lease, locator, enrollment.

## 3. Window anatomy

One default layout, modeled on Unity's default and Unreal's level editor.

```
┌─ File  Edit  Assets  Object  Component  Window  Help ──────── project · scene* ─┐
├─ [Select Move Rotate Scale] [Pivot ▾][Local ▾][Snap ▾]   ▶  ❚❚  ▶❘   [Layout ▾] [AI] ─┤
├────────────┬──────────────────────────────────────────────┬────────────────────┤
│ Hierarchy  │ Scene │ Game                                 │ Inspector          │
│            │  (viewport; view options overlaid inside it) │                    │
├────────────┴──────────────────────────────────────────────┴────────────────────┤
│ Assets │ Console                                                               │
├─ status message                                   progress · warnings · errors ─┤
```

- **Menu bar.** File, Edit, Assets, Object, Component, Window, Help. Preferences
  and Project Settings are under Edit. Panels and layouts are under Window.
- **Toolbar.** One row, always. Tools on the left, Play controls centered,
  layout picker and the AI indicator on the right. Nothing else.
- **Viewport overlays.** Projection, view direction, shading mode, grid, camera
  speed and stats live inside the viewport as small overlaid controls, with an
  orientation gizmo in the top-right corner. They are not toolbar rows.
- **Panels** are real docked panels with tabs. They can be dragged to another
  dock, closed and reopened from Window, and the layout is saved. Use an
  existing egui docking crate; do not write one.
- **Status bar.** One line. Left: the latest message. Right: background task
  progress with Cancel, then warning and error counts that open the Console.
- **Narrow windows.** The editor must stay usable at 960×1080, because the
  side-by-side AI workflow gives it half a screen. Below about 1100 px wide,
  Hierarchy and Inspector share one tabbed dock and the toolbar overflows into
  a `»` menu. Minimum window size is 960×600.

Today's main violations, for orientation: three toolbar rows; Hierarchy and
Inspector painted over the viewport instead of docked; no Console; preferences
under File; Reset required before Play; paragraphs of helper text in panels.

## 4. Sizes and spacing

Use tokens from the UI kit, never raw numbers in panel code.

| Token | Value | Use |
| --- | --- | --- |
| Spacing scale | 2, 4, 8, 12, 16, 24 px | All gaps and padding |
| Control height | 24 px | Buttons, fields, dropdowns |
| Row height | 24 px | Property rows, list and tree rows |
| Toolbar height | 36 px | Main toolbar |
| Menu row | 26 px | Menu and context menu items |
| Panel tab | 28 px | Dock tab strip |
| Icon | 16 px, 20 px in the toolbar | All icons |
| Minimum hit target | 24×24 px | Anything clickable |
| Corner radius | 4 controls, 8 menus, 10 windows | Both styles |
| Body text | 13 px | Labels, values |
| Small text | 12 px | Secondary detail, status bar |
| Panel title | 13 px semibold | Tab labels, section headers |
| Tree indent | 14 px per level | Hierarchy, folder tree |
| Focus ring | 2 px accent, 1 px offset | Keyboard focus |

Ship one UI font (Inter, SIL OFL) and one monospace font for the Console and
code. The UI scale preference multiplies everything.

Today the editor uses 30–32 px controls and 14 px text with egui's default
font, which is why it shows so little and feels heavy.

## 5. Components

Build each of these once in the UI kit and use it everywhere. A panel that
calls `ui.button` or `ui.label` directly for one of these is a defect.

- **Button.** Text, Title Case, verb first ("Import", "Build and Run"). One
  primary button per dialog at most. Destructive buttons are never primary.
- **Icon button.** 24×24, tooltip "Name (Shortcut)". Toggle state shown by a
  filled background, not by color alone.
- **Property row.** Label on the left in a fixed column (about 38% of the
  panel), control on the right filling the rest. One row per property.
  A vector is one row: `Position  X [  ] Y [  ] Z [  ]`.
- **Number field.** Click to type, Enter commits, Escape cancels, Tab moves on.
  Drag the label or axis letter to scrub; Shift scrubs fast, Alt slow. Accept
  simple math (`2*3`, `+=1`). Show units as a suffix. Mixed values show `—`.
- **Section header.** A collapsible row: chevron, icon, title, optional enable
  checkbox, and a `⋮` menu on the right with Reset, Copy, Paste and Remove.
  Used for every component in the Inspector.
- **Reference field.** Icon and name of the linked asset or object, a picker
  button, accepts drag and drop, click reveals the target.
- **Dropdown, checkbox, slider, color field.** Standard forms only.
- **Tree and list rows.** Chevron, type icon, name. Full-row selection
  highlight. Hover reveals row actions (visibility, lock). Rename in place
  with F2. Long lists are virtualized.
- **Search field.** Magnifier icon, placeholder "Search", clear button,
  Ctrl+F focuses it, Escape clears then leaves.
- **Context menu.** Every object, asset, component header and property has
  one, containing what the reference engines put there.
- **Dialog.** Title, content, button row at the bottom right with Cancel left
  of the primary action. Enter activates the primary action, Escape cancels.
  Focus starts in the first field. File choosing uses the native OS dialog.
- **Toast.** Bottom-right, one line, disappears after four seconds, may carry
  one action such as Undo. For results of things the creator just did.
- **Empty state.** Centered small illustration, one line of text, one action.
- **Progress.** In the status bar with Cancel. Never block the window for
  work that takes longer than a frame.

## 6. Panels

**Hierarchy.** Tree of the scene's objects. `+` button and search in the
header. Drag to reparent or reorder, with an insertion line. Click selects,
Ctrl-click toggles, Shift-click selects a range. Double-click or F frames the
object in the viewport. Eye and lock toggles appear on hover. Right-click
gives Create, Duplicate, Rename, Delete, Copy, Paste, Frame.

**Inspector.** Shows the selection. Header: icon, editable name, enabled
checkbox. Transform is always the first section and is three rows. Each
component is a collapsible section. An **Add Component** button with a search
popup sits at the bottom. Multiple selection edits shared properties.
Selecting an asset shows its import settings and a preview. No prose: a
property's explanation is its tooltip, and a real problem is one inline
warning line with an icon and an action.

**Assets.** Folder tree on the left, grid of thumbnails on the right, with
breadcrumbs, search and a size slider. The project's asset folder on disk is
the source of truth: files dropped in the folder or onto the panel are
imported automatically, and changed files are reimported automatically. Drag
an asset into the scene, the Hierarchy or a reference field. Right-click
gives Create, Rename, Duplicate, Delete, Reimport and Show in File Manager.
There is no "import with an ID" form.

**Scene and Game.** Scene is the editing view; Game is what the player sees.
Navigation follows Unity: Alt+left-drag orbits, middle-drag pans, scroll
zooms, right-drag looks around, and holding the right button enables WASD
fly with Q and E for down and up and Shift for speed. F frames the
selection. Drag on empty space box-selects. W, E and R switch Move, Rotate
and Scale; Q is Select.

**Console.** Every log line, warning and error from scripts, imports and the
engine. Clear, Collapse, and level filters with counts. Selecting an entry
shows its full text; double-clicking a script error opens the script at that
line.

**Play controls.** Play starts the game from the current scene immediately.
Stop returns to editing. While playing, the Play button is filled and the
views carry a thin accent border. There is no separate Reset step. Until the
running game can be edited live (Phase 2), an edit made during Play applies
to the scene and restarts Play.

## 7. Keyboard and focus

- Every control is reachable with Tab and Shift+Tab in reading order, inside
  every panel and dialog. F6 and Shift+F6 move between panels. The viewport
  surface, panel backgrounds and resize handles are not Tab stops.
- Keyboard focus is always visible.
- Text fields own typing: single-key shortcuts never fire while a field has
  focus. Escape first cancels the current edit, then closes the popup or
  dialog.
- Every command has a name, appears in the Shortcuts editor, shows its
  shortcut in its menu item and tooltip, and can be remapped.
- The command palette (Ctrl+K) searches every command and asset.

Default shortcuts, following Unity. Confirm against the engines' shortcut
documentation when implementing; existing bindings that already match stay.

| Action | Default | Action | Default |
| --- | --- | --- | --- |
| Select / Move / Rotate / Scale | Q / W / E / R | Play | Ctrl+P |
| Frame selection | F | Pause | Ctrl+Shift+P |
| Duplicate | Ctrl+D | Step | Ctrl+Alt+P |
| Delete | Delete | Save | Ctrl+S |
| Rename | F2 | Save As | Ctrl+Shift+S |
| Undo / Redo | Ctrl+Z / Ctrl+Y and Ctrl+Shift+Z | Build and Run | Ctrl+B |
| Select all | Ctrl+A | Command palette | Ctrl+K |
| Find in panel | Ctrl+F | Preferences | Ctrl+, |
| New empty object | Ctrl+Shift+N | Hold to snap while dragging | Ctrl |

## 8. Writing

- **Case.** Title Case for menu items, buttons, panel titles and property
  labels, as Unity and Unreal do. Sentence case for messages, tooltips and
  descriptions.
- **Ellipsis.** A command that opens a dialog asking for more input ends in
  `…`.
- **Length.** Status messages and toasts fit on one line, about 60 characters.
- **Words.** Use §2. Say what happened in the creator's terms.
- **Errors.** `Couldn't <do the thing>. <Why>. <What to do next>.` Put detail
  in the Console, not the message.
- **No instructions on screen.** Do not describe how the software works under
  a control. If people need teaching, that belongs in a tooltip or the docs.

Real examples from the current editor and their replacements:

| Now | Should be |
| --- | --- |
| "Place creates an editable scene copy. Refresh keeps placement and uses the last accepted model if refused." | Nothing. Dragging a model into the scene is self-evident. |
| "Model beacon accepted. Use Place in Content to create a scene copy; Refresh preserves existing placements. Preview updates on Reset." | Toast: "Imported beacon.glb" |
| "Authored scene/content changed; runtime cancelled. Reset to run the current data." | Nothing. Play always runs the current scene. |
| "Mesh assigned to {id} only, revision{revision}; Undo restores the assignment, preview unchanged until Reset." | Nothing. The change is visible and Undo is in the Edit menu. |
| "Transform fields: local to parent. Gizmos: world-space placement." | A Local/World toggle in the toolbar. |
| "Ready. Right-drag to orbit; scroll to zoom." | An empty status bar. |
| "New scene", "Open scene…", "Switch Project…" in one menu | "New Scene", "Open Scene…", "Open Project…" |

All user-facing text goes through one string table with a key per string.
New code must not contain inline user-facing literals. This is also what
makes the Rot wording optional and translation possible later.

## 9. Rot and Plain

The interface has two styles, chosen at first launch and under
Preferences → Appearance:

- **Rot** (default). The brand style.
- **Plain.** A neutral style comparable to Unity or Unreal for people who want
  no theme.

Each has Dark and Light modes (and System). A separate checkbox, **Themed
wording**, is on by default in Rot and controls the playful copy, so a
creator can keep the Rot look with plain text.

### Color

Colors are semantic tokens. Panel code asks for `surface` or `accent`, never
a hex value. The Rot palette is the one already in
`crates/rot-editor/src/editor_theme.rs`; keep it and add the missing roles.
All four palettes below pass WCAG contrast: 4.5:1 for text, 3:1 for control
boundaries (checked 2026-11-17).

| Role | Rot dark | Rot light | Plain dark | Plain light |
| --- | --- | --- | --- | --- |
| background | `#151314` | `#EEE6E1` | `#1B1B1C` | `#ECECEE` |
| surface | `#201D1E` | `#F7EFEB` | `#242425` | `#F6F6F7` |
| raised | `#292527` | `#FFFFFF` | `#2D2D2F` | `#FFFFFF` |
| field | `#191617` | `#FFFFFF` | `#161617` | `#FFFFFF` |
| divider | `#383032` | `#D9CEC8` | `#37373A` | `#D5D5D9` |
| border | `#827574` | `#8A7B77` | `#7A7A80` | `#85858C` |
| text | `#F1E8E3` | `#151314` | `#E6E6E8` | `#1A1A1C` |
| text secondary | `#B9ADAA` | `#5C504D` | `#A9A9AE` | `#55555C` |
| accent | `#EF9AA2` | `#96373E` | `#5AA2EA` | `#1A62B6` |
| text on accent | `#1B1012` | `#FFFFFF` | `#0B1622` | `#FFFFFF` |
| selection | `#583439` | `#F0D9DC` | `#1F3B57` | `#CFE3FA` |
| success | `#9BDDAB` | `#2F6B3F` | `#6CC38A` | `#1E7A3E` |
| warning | `#FFC078` | `#8A5A00` | `#E8B454` | `#8A5A00` |
| error | `#FF8291` | `#B3261E` | `#F0736B` | `#B3261E` |

In Rot dark the error color is close to the accent. Never rely on color
alone: errors and warnings always carry an icon and text.

**Rot red** `#FA2934` is the "alive" color. Use it only for things that are
live: the AI indicator while connected, the selection outline in the
viewport, Play mode. It is not a text color.

### The motif

Rot means decay feeding new growth: fungus, mycelium, moss, compost. Lean on
it lightly. A creator should notice it in the corners, not in their way.

- **Where it appears.** The mushroom mark (app icon, project hub, About, the
  AI indicator). Empty states. The project hub and splash background. The
  loading indicator (three pulsing "spores"). The line shown when a build or
  a long task completes. The About box and the welcome screen.
- **Where it never appears.** Inside working panels, behind text, on
  property labels, in the viewport, in error or warning states.
- **Artwork.** Thin line drawings of mycelium threads or a small mushroom, at
  low contrast (within 6% of the surface color). No textures behind controls.
- **The AI indicator.** The mushroom is grey when no AI app is connected, Rot
  red when one is, and pulses slowly while the AI is making changes.
- **Motion.** 120 ms ease-out for hover, press and open. Nothing loops except
  the loading indicator and the AI pulse. Honor a reduced-motion preference.

### Themed wording

Rot wording is dry, short and occasional: at most one themed line on screen
at a time.

Themed wording is allowed in empty states, loading lines, completion lines,
the welcome screen, About and tips. It is never used in menu items, button
labels, property names, errors, warnings, confirmations, anything about
saving or losing work, or anything about AI permissions. It never replaces
the functional word: a delete toast says "Deleted", in both styles.

Starting set. Every key has both; the user will refine the Rot column.

| Moment | Plain | Rot |
| --- | --- | --- |
| Welcome | Welcome to Rot Engine | Welcome to Rot Engine. Let's grow something. |
| No recent projects | No recent projects | Nothing planted yet. |
| Empty Hierarchy | This scene is empty. Add an object with +. | Nothing growing here yet. Add an object with +. |
| Empty Assets | No assets yet. Drop files here to import. | Bare soil. Drop files here to import. |
| Empty Console | No messages | All quiet in the undergrowth. |
| Opening a project | Opening project… | Spreading mycelium… |
| Importing | Importing 12 files… | Breaking down 12 files… |
| Building | Building… | Fruiting… |
| Build finished | Build finished in 12 s | Fruited in 12 s. |
| AI app connected | Claude Code connected | Claude Code has taken root. |
| Search with no results | No results | Nothing under this log. |
| About tagline | (none) | Everything grows from rot. |

## 10. UI review checklist

Review the applicable checks on screenshots of the affected UI. Use both
1280×800 and 1920×1080 for layout and scaling changes. For a change confined
to one control, use a representative size and check any affected appearance
variants.

1. Would this surface look at home in Unity or Unreal? Is each control where
   a user of those engines would look first?
2. Count the permanent controls. Can any move into a menu, a context menu or
   a section that starts collapsed?
3. Is there any sentence explaining how the software works? Remove it.
4. Are all labels from §2, in Title Case, with nothing internal showing?
5. Do rows, labels and fields line up on a grid? One row per property?
6. Are sizes and gaps from §4, and colors from tokens?
7. Does `tabwalk` visit every control in order, with a visible focus ring?
   Do Enter and Escape do the standard thing?
8. Does every control have a tooltip naming it and its shortcut?
9. Is the empty state designed? The error state? The loading state?
10. Does it still work at 960×1080 and at UI scale 1.5?
11. Do the affected appearance variants keep text, control states and
    focus clear?
12. Does it stay smooth with a 5,000-object scene?
