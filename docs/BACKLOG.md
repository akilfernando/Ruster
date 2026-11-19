# Backlog

The ordered work for the [parity plan](PARITY-PLAN.md). Take the first open
package in your lane under **Now**. How to work is in the
[PLAYBOOK](PLAYBOOK.md); UI rules are in [UX-STANDARDS](UX-STANDARDS.md).

- **Size.** S is half a session, M is one session, L must be split into M
  packages before starting.
- **Brief.** *ready* means implement as written. *brief needed* means the
  strongest available model writes the Do and Done-when lists first.
- **Status.** `open`, `in progress — who, date`, or `done — date, commit`.
- File references were checked on 2026-11-17 and drift; search if a line
  number is off.

## Needs the user

- **Vetoes.** The [architecture decisions](PARITY-PLAN.md#4-architecture-decisions)
  and the [vocabulary](UX-STANDARDS.md#2-vocabulary) are agent-selected.
- **Outside the repository.** The installed editor is still the pre-rename
  build. Two MCP entries in `~/.codex/config.toml` point at old names. Projects in
  `~/Projects/Ruster-Creations` use pre-rename file names.
- **For the user (2026-11-17).** A-04's real-config test left
  `~/.claude.json.rot-backup` and `~/.codex/config.toml.rot-backup` (safe to
  delete) and an empty `"mcpServers": {}` in `~/.claude.json`; no `rot`
  entry remains in either config. The current build is installed
  (607b908); to connect an AI app, use Window ▸ AI Connection… ▸ Connect an
  AI App… from the installed editor.
- **Answered 2026-11-17 (user).** A-04 may add the `rot` entry to the user's
  real Claude Code and Codex configs (backup, that entry only, Undo). P-02 may
  replace the installed pre-rename build (old one kept for `--rollback`).
  Phase 1 runs as three parallel lanes.

## Inbox

One line each. Anyone may add; the user or a planning pass orders them.

- Grouping is blocked scene-wide by input actions, a HUD, ordinary script
  sources, or object/application script attachments. C-02/C-03 must preserve
  these features alongside hierarchy during migration. Until then, explain
  the specific blocker in the editor.
- Human walkthrough: New Group creates an empty group with multiple objects
  selected. Add a discoverable Group Selection command and support dragging
  rows onto a group (C-05).
- Human walkthrough: material changes are drafts until Apply Material.
  Make the pending state and commit action clearer.
- Human walkthrough: ChatGPT requested several write approvals after
  connecting. Consider batched undoable edits in C-06 to reduce tool calls.
- `ui_drive.py` screenshots do not change after key-only input (Tab): the hub and the editor gave byte-identical PNGs across Tab presses while the accessibility focus moved, so focus rings cannot be checked from screenshots on the hidden workspace (S-13, 2026-11-17).
- `third_party/egui_tiles` is egui_tiles 0.18.0 with three Sense lines patched so dock dividers and tab-bar backgrounds are not Tab stops; drop it when upstream allows that (S-07a).
- MCP tool list is 145 KB for 98 tools against a 40 KB Phase 1 budget
  (P-01, 2026-11-17). Shorten descriptions and merge tools.
- An open scene repaints about 3 frames per 10 s while idle; the hub draws
  none (P-01, 2026-11-17).
- Scrubbing an Inspector axis letter commits on release; the viewport does not follow live during the drag (S-02).
- `conflict_never_reads_back_and_mismatched_readback_fails_closed` (connection/worker.rs) failed once in about 105 full-suite runs under parallel load: the conflict case reported "failed or indeterminate". Not reproduced; a spawn `ETXTBSY` on the freshly written script is the likely cause (2026-11-17).
- `enrolled_local_transport_never_needs_native_approval_and_revokes_before_commit` (bridge.rs:534) failed once in 24 full-suite runs under parallel load: `grant.revoke(false)` returned `WouldBlock`. Decide whether a contended durable revoke may fail or must wait (2026-11-17).
- The legacy logistics prototype (`rot-core`, `rot-cli`, `rot-viewer`,
  `rot-script`, `examples/yard-app`, editor `ui.rs` and `model.rs`, five
  legacy MCP tools) is unused by the scene editor. Proposal: delete it.
  Needs the user's yes.
- The registry and enrollment connection modes (`ROT_MCP_PROJECT_REGISTRY`,
  `ROT_CONNECTOR_ENROLLMENT`, `scripts/enroll_local_connector.py`,
  `support/enrollment.rs`, `support/project_startup.rs`) are superseded by
  A-01/A-02 and still pin binaries. Proposal: delete them (agent, 2026-11-17).
- `docs/branding/rot/README.md` describes a monogram; the app uses the
  mushroom. Refresh the brand note once the user confirms the mark.
- In the Rot dark palette the error color is close to the accent color.
- Decision 0050 says save and build destinations are granted in the app;
  the code only reads environment variables. A-02 resolves this.

## Now — Phase 1: Shell

Three lanes that can run at the same time: **UX** (S and T packages), **AI**
(A packages), **Tools** (P packages). With no lane given, take the UX lane;
it is the user's stated weak point. The lanes mostly touch different files.
The exception is `crates/rot-editor/src/scene_editor.rs`, which several
packages edit: keep changes there small and merge to the trunk often.

---

### UX lane

#### S-01 · Tab reaches every panel — S — ready
Status: done — 2026-11-17, 9fc6522
**Outcome.** Tab and Shift+Tab move through every control in the Hierarchy
and Inspector panels.
**Facts.** Today focus goes menu → Assets controls → toolbar → "Fit", then
drops to the window and wraps to File; the side panels are never entered.
The 2026-11-16 build behaves the same. Known invisible focus stops: the
viewport surface (`crates/rot-render/src/lib.rs:1031`,
`Sense::click_and_drag()`), each panel backdrop
(`crates/rot-editor/src/scene_editor/sidebars.rs:104`), each resize handle
(`sidebars.rs:136`), and the gizmo's 1×1 widget
(`third_party/transform-gizmo-egui/src/lib.rs:87`). Why focus never reaches
the panel contents was not found.
**Do.** Reproduce with
`python3 scripts/ui_drive.py --scene examples/arrangement.scene.json "click:left" tabwalk:60`
(clicking an object row fills the Inspector). If the cause is not obvious,
log `ctx.memory(|m| m.focused())` per frame. Make surfaces that are not
controls non-focusable. Fix whatever blocks the panels. This package is
only the focus fix; do not restyle anything.
**Done when.** `tabwalk` visits the Hierarchy search field, the object rows
and the Inspector fields, each panel's controls together and top to bottom,
then returns to File; no stop is an unlabeled surface; Shift+Tab reverses;
a `shot:` after a few Tab presses shows the focus ring on a Hierarchy
control and on an Inspector field; the order is pasted in the commit
message.

#### S-02 · UI kit, tokens and fonts — M — ready
Status: done — 2026-11-17, see git log
**Outcome.** One module supplies every control, size and color, and the
editor becomes denser and consistent.
**Facts.** `crates/rot-editor/src/editor_theme.rs` has a 17-role `Palette`
(`:51`) and sizes set inline in `apply()` (`:156-193`): 30–32 px controls,
14 px default egui font. Helpers exist for dialogs and a primary button
only. Panels call `ui.button` about 150 times and `ui.label` about 320.
**Do.** Create `crates/rot-editor/src/ui_kit/` with tokens
([sizes](UX-STANDARDS.md#4-sizes-and-spacing)), the palette roles
([colors](UX-STANDARDS.md#color)), and the
[components](UX-STANDARDS.md#5-components): button, icon button, property
row, vector row, number field with drag-to-scrub, section header, reference
field, search field, empty state. Bundle Inter and a monospace font and add
their license notices the way other third-party notices are recorded.
Apply the new sizes globally. Convert the Inspector's Transform block to
three vector rows as the first real use.
**Done when.** Screenshots at 1280×800 show 24 px controls, Inter at 13 px
and Transform as three rows; nothing clips at 960×600 or UI scale 1.5;
the gate passes.
**Not now.** Converting every panel. Panels move to the kit as each is
rebuilt in later packages.

#### S-03 · Menu bar and names — S — ready
Status: done — 2026-11-17, see git log
**Outcome.** Menus are where a Unity or Unreal user looks, with standard
names.
**Facts.** Menus are built in `scene_menu_contents`
(`crates/rot-editor/src/scene_editor.rs:5304-5605`). Today: File, Create,
Edit, View, Content, Help, mixed capitalization, preferences under File.
**Do.** Build these menus, Title Case, each item showing its shortcut:
- **File:** New Scene, Open Scene…, Open Recent ▸, Save, Save As…, —,
  New Project…, Open Project…, —, Build…, Build and Run, —, Exit.
- **Edit:** Undo, Redo, —, Copy, Paste, Duplicate, Rename, Delete, —,
  Select All, Frame Selected, —, Play, Pause, Step, —, Project Settings…,
  Preferences….
- **Assets:** Import…, Create ▸, Refresh.
- **Object:** Create Empty, 3D Object ▸ (Cube, Sphere, Capsule), Light ▸
  (Directional Light, Point Light), Camera…, Game UI….
- **Window:** one checkable item per panel, AI Connection, Play Checks,
  Script Debugger, Recovery Checkpoints, —, Layouts ▸.
- **Help:** Documentation, Keyboard Shortcuts…, About Rot Engine.
Rename panel titles to Hierarchy, Assets, Scene and Game.
**Done when.** The menus match this list in screenshots; every old command
is still reachable; compact-width collapsing still works.
**Not now.** A Component menu (arrives with components in Phase 2).
**Left for later (agent, 2026-11-17).** Build and Run (C-09), Assets → Refresh
(C-04), Play/Pause/Step in Edit (S-05), Script Debugger in Window (no
standalone window yet), real Project Settings… and Preferences… windows (S-10;
today Project Settings ▸ Input Actions… and Preferences… open the old modal).

#### S-04 · One toolbar; view options move into the viewport — M — ready
Status: done — 2026-11-17, see git log
**Outcome.** One toolbar row and a much larger viewport.
**Facts.** Toolbar rows are laid out in `scene_editor.rs:5799-5842`; a third
row ("Perspective", "View presets", "Fit") is drawn inside
`rot_render::allocate_viewport` (`crates/rot-render/src/lib.rs:983-1029`).
At 1280×800 the viewport is about 745×425.
**Do.** Follow the [window anatomy](UX-STANDARDS.md#3-window-anatomy). Left:
Select, Move, Rotate, Scale, then Pivot, Local/World and Snap as compact
dropdowns. Center: Play, Pause, Step. Right: Layout and the AI indicator.
Scene and Game become a tab strip above the viewport. Projection, view
presets and Frame become small overlaid controls in the viewport's top-left
corner. Move the "Runtime" menu's items to Window and Help. Overflow into a
`»` menu when narrow.
**Done when.** Exactly one toolbar row at every width from 960 px up; the
viewport at 1280×800 is at least 520 px tall; `tabwalk` still reaches
everything.

#### S-05 · Play, Pause, Step, Stop — M — ready
Status: done — 2026-11-17, see git log
**Outcome.** Pressing Play runs the current scene. There is no Reset.
**Facts.** The runtime toolbar is in
`crates/rot-editor/src/behavior_ui.rs:2386-2511`. Play is enabled only after
Reset (`ready`, `:2422`, set in `complete_reset` `:2134`); any edit clears
it (`:2010-2017`). "Control runtime" hands the keyboard to the game
(`behavior_ui/runtime_input.rs:124`).
**Do.** Play resets from the current scene and starts in one action. Stop
returns to the editing view. Step from stopped starts paused on the first
tick. An edit made during Play applies to the scene and restarts Play.
Clicking the Game view while playing gives it the keyboard; Escape gives it
back. Shortcuts Ctrl+P, Ctrl+Shift+P, Ctrl+Alt+P. A thin accent border marks
the views while playing. The MCP play tool gets the same one-step behavior.
**Done when.** New scene, add a script, press Play: it runs. No "Reset",
"Runtime" or "Control runtime" label remains. Stop restores the edit view.
**Not now.** Editing the running game's live values (Phase 2).
**Notes (agent, 2026-11-17).** No MCP tool starts Play today (play checks run
headless), so there was nothing to change on the MCP side. About 30 status
messages still mention Reset; S-08 rewrites them. Focus Game Controls moved
from Ctrl+Shift+P to Ctrl+Shift+G so Pause can use Unity's key.

#### S-06 · Console, status bar and toasts — M — ready
Status: done — 2026-11-17, see git log
**Outcome.** Messages have a home. The status bar is one quiet line.
**Facts.** There is no Console; logs go to stderr
(`crates/rot-editor/src/main.rs:47-62`). The status bar is `asset_status_ui`
(`scene_editor/asset_commands.rs:2343`). Panels set long `self.notice` and
`self.error` strings directly.
**Do.** Add one message log (level, source, text, detail) kept in memory.
Build the [Console](UX-STANDARDS.md#6-panels) as a tab beside Assets. Send
script output and errors (with file and line), import warnings and build
output to it. Status bar: latest message on one line, progress with Cancel,
warning and error counts that open the Console. Add toasts for results of
the creator's own actions. Convert the twenty most frequent notices.
**Done when.** A script error shows in the Console with its file and line;
double-clicking opens the configured code editor; the status bar never
wraps at 960 px.

#### T-01 · Rot and Plain styles — M — ready
Status: done — 2026-11-17, see git log
**Outcome.** A creator picks Rot or Plain, dark or light, and can switch at
any time.
**Facts.** `ThemeChoice` has System, Dark, Light and OLED black
(`editor_theme.rs:13-46`). The user decided on 2026-11-13 to keep OLED as an
option. About 33 hard-coded `Color32` values remain (for example
`crates/rot-render/src/lib.rs:1067`).
**Do.** Add Style (Rot, Plain) beside Mode (System, Dark, Light, OLED black).
Add the Plain palettes and missing roles from the
[color table](UX-STANDARDS.md#color). Add a first-launch chooser showing both
styles. Add the Themed Wording checkbox (it takes effect in S-08). Replace
hard-coded colors in the files you touch.
**Done when.** Switching is instant; screenshots of all four palettes pass
the checklist; layout is pixel-identical between styles.

#### S-07a · Dock shell: every panel in a tab — M — ready
Status: done — 2026-11-17, see git log
**Outcome.** Hierarchy, Inspector, Scene, Game, Assets and Console are tabs
in one dock. Tabs drag between docks; nothing is painted over the viewport.
**Facts.** Agent decision 2026-11-17: `egui_tiles = "0.18.0"` (needs `egui
^0.37.0`, as the repo has; `egui_dock` 0.21.1 stops at 0.36). Sidebars
overlay the viewport (`scene_editor/sidebars.rs`); Assets is `Panel::bottom`
and the viewport is the `CentralPanel` closure, both in `workspace_ui`.
`rot_render::allocate_viewport` fills the `Ui` it is given and
`render_viewport_with_materials` adds an `egui_wgpu` paint callback for that
rect, so the Vulkan view works in a tile unchanged. There is one render
surface. egui_tiles calls `pane_ui` only for a container's active tab.
**Do.** Add the crate; rerun `scripts/dependency_notices.py`. Create
`crates/rot-editor/src/panels/mod.rs`, declared in `scene_editor.rs` with
`#[path = "panels/mod.rs"]` so it reaches `SceneEditor`'s private fields:
`enum Pane` (six variants, serde); `default_tree()` = Vertical[ Horizontal[
Hierarchy 0.14, Tabs[Scene, Game] 0.59, Inspector 0.27 ] 0.75, Tabs[Assets,
Console] 0.25 ]; a `Behavior<Pane>` over `&mut SceneEditor` (hold the tree
in an `Option` and `take()` it per frame), `all_panes_must_have_tabs: true`,
tab colors from the theme. Move the `CentralPanel` closure body to
`viewport_pane_ui`, used by Scene and Game: their active tab drives
`set_player_preview` (replacing S-04's strip) and neither is draggable
(`is_tile_draggable`), so one surface is enough. Delete `SceneSidebars`,
`sidebar_budget`, the `content_browser` panels and the `sidebar_pointer`
terms. Block viewport input while `tree.dragged_id` is some. In
`ai_workspace` mode show `viewport_pane_ui` without the tree until S-07c.
**Done when.** Screenshots at 1280×800 and 1920×1080 match the
[window anatomy](UX-STANDARDS.md#3-window-anatomy); clicks near each edge of
the Scene tab pick correctly; the camera survives a divider drag; in-process
tests show the wheel over Hierarchy does not zoom and a pointer drag moves
Console into the Inspector dock (`ui_drive.py` cannot drag; say so);
`tabwalk` reaches every panel.
**Not now.** Close buttons, saving and layouts (S-07b, S-07c).

#### S-07b · Close, reopen, saved layout and F6 — M — ready
Status: done — 2026-11-17, see git log
**Outcome.** Panels close and reopen from Window, the layout survives a
restart, and F6 moves the keyboard between panels.
**Facts.** egui_tiles: `Tree::set_visible` hides a tile in place,
`on_tab_close` returning false keeps it, `make_active` selects a tab,
`on_edit` fires on each user change, `Tree<Pane>` is serde. Preferences are
`~/.config/rot/editor.json` (`Workspace::load`, `workspace.rs`;
`ROT_EDITOR_STATE` overrides). `Preferences::parse` rejects unknown keys and
P-02 keeps the old build for rollback, so the layout cannot go in that file.
**Do.**
- Save `layout.json` beside `editor.json` (`state_path().parent()`):
  `{"format":"rot-editor-layout","version":1,"layouts":{"default":<Tree>}}`.
  Mark dirty in `on_edit`; write by temp file and rename where
  `workspace.flush()` is called and on exit. A missing or bad file, or a
  tree without each `Pane` exactly once, loads `default_tree()` with one
  Console warning.
- Hierarchy, Inspector, Assets and Console get a close button
  (`set_visible(false)`). Scene and Game do not close.
- Window's panel items show `tree.is_visible`; checking one shows, activates
  and focuses the tab. `Action::FindObjects` and `FindContent` do the same.
  Add Window ▸ Layouts ▸ Reset Layout.
- Keep the seven sidebar keys in `editor.json` read and written but unused;
  remove their settings controls.
- Add `Action::NextPanel` (F6) and `PreviousPanel` (Shift+F6): the next
  visible dock in reading order (`tiles.rect`), focusing the first control
  of its active tab (pattern: `hierarchy_focus`).
**Done when.** Closing Hierarchy unchecks it in Window and checking it
restores it to the same dock; a test saves a changed tree through a
temporary state path and a new editor loads it; a corrupt file opens the
default; F6 visits each dock once and wraps, and is remappable.

#### S-07c · Default and Side by Side layouts; no AI workspace — M — ready
Status: done — 2026-11-17, see git log
**Outcome.** Two layouts, picked from the toolbar or by window width. The
editor beside an AI app is the same editor.
**Facts.** The AI workspace is the `ai_workspace` flag with `ai_header`,
`ai_tools` and `ai_drawer_ui` (`scene_editor/ai_workspace.rs`), entered from
File ▸ Advanced or `ROT_WORKSPACE=ai` (host setup in `scene_editor.rs`,
which also skips the project picker). About a dozen `self.ai_workspace`
branches hide menus, toolbar and status bar. It has a test in
`ai_workspace_tracer.rs` and a section in `crates/rot-editor/README.md`.
**Do.**
- `layout.json` gains `"active"` and a `side_by_side` tree: Horizontal[
  Vertical[ Tabs[Scene, Game] 0.62, Tabs[Assets, Console] 0.38 ] 0.64,
  Tabs[Inspector, Hierarchy] 0.36 ]. Each layout keeps its own arrangement.
- S-04's Layout picker and Window ▸ Layouts list Default and Side by Side
  as radio items, then Reset Layout for the active one.
- Agent decision: crossing 1100 px of window width switches to Side by Side
  below and Default above. A manual choice holds until the next crossing.
- Delete `Drawer`, `ai_drawer_ui`, `ai_header`, `ai_tools`,
  `set_ai_workspace`, their fields, the menu item and every branch.
  `ROT_WORKSPACE=ai` only starts in Side by Side and skips the picker.
  Update the tracer test and the README.
**Done when.** Screenshots at 960×1080, 960×600, 1280×800 and 1920×1080 clip
nothing; at 960×1080 the viewport is at least 560×560 px; `resize:` from
1280 to 960 and back switches layouts and restores each arrangement; both
survive a restart; no `ai_workspace` identifier remains in the editor crate.
**Not now.** User-named layouts, floating OS windows, multiple viewports.

#### S-07d · Panel code in `panels/<name>.rs` — M — ready
Status: done — 2026-11-17, see git log. Hierarchy, Inspector and viewport moved; Assets and Console already had files.
**Outcome.** Each panel's code is one file and `scene_editor.rs` is about
1,500 lines shorter. No behavior changes.
**Facts.** Panel bodies are `SceneEditor` methods in `scene_editor.rs`:
`hierarchy_contents`, `inspector_scroll_ui` and `inspector_contents`,
`browser_contents`, and `viewport_pane_ui` from S-07a. Several open packages
edit the same file.
**Do.** Move them, with their private helpers and tests, as `impl
SceneEditor` blocks into `panels/hierarchy.rs`, `inspector.rs`, `assets.rs`,
`console.rs` and `scene_view.rs` (Scene and Game). Delete `sidebars.rs` and
`ai_workspace.rs` if S-07a and S-07c left anything in them. Code motion only:
no renames, no logic edits. Start from a fresh trunk, finish in one sitting
and merge at once. Suits a cheaper model.
**Done when.** `git diff --color-moved` shows moves only; `scene_editor.rs`
holds no panel body; one `ui_drive.py` screenshot is unchanged; the gate
passes.

#### S-08 · String table and wording pass — M — ready
Status: wording sweep landed 2026-11-19. Empty Hierarchy, project opening and build states use the shared table. Follow-up on 2026-11-19 simplified recovery, Play Checks, script editing and runtime copy. A direct widget-label scan found no single-line literals over 80 characters. Indirect and generated messages still need coverage before full acceptance.
**Outcome.** All text follows the [writing rules](UX-STANDARDS.md#8-writing)
and comes from one table with Plain and Rot wording.
**Facts.** Strings are inline. About 337 literals are longer than 80
characters, many of them contract explanations.
**Do.** Add a string table keyed by an enum, with a Plain column and an
optional Rot column. Move user-facing text into it surface by surface:
menus, toolbar, Hierarchy, Inspector, Assets, dialogs, messages. Delete
helper prose, shorten messages, apply the vocabulary and Title Case, move
real detail into tooltips. Update tests that assert old strings. This is
mechanical and suits a cheaper model once the first surface sets the
pattern.
**Done when.** Fewer than twenty UI string literals over 80 characters
remain outside tests, all of them tooltips; none of the banned internal
words appear; the Themed Wording checkbox changes the lines that have a Rot
variant.

#### S-09 · Command layer and palette — M — ready
Status: done — 2026-11-17, see git log
**Outcome.** Every action is one registered command used by menus, context
menus, shortcuts and a command palette.
**Facts.** Shortcuts use an `Action` enum
(`crates/rot-editor/src/workspace.rs:14-49`, defaults `:190-221`) dispatched
in `process_queued_action` (`scene_editor.rs:5017`). Context menus call
`object_action("duplicate")` directly with their own labels. MCP has a third
path (`execute`, around `scene_editor.rs:3237`).
**Do.** Add a command registry: id, label key, default shortcut, enabled
test, run function, Undo label. Move every `Action` and every menu-only
action into it. Menus, context menus and toolbar buttons call commands by
id. Add the palette on Ctrl+K.
**Done when.** No menu item or context-menu entry has its own handler or
label; the palette can run every command.
**Not now.** Routing MCP through commands (C-06).

#### S-10 · Preferences, Project Settings and Shortcuts — M — ready
Status: done — 2026-11-18, see git log
**Outcome.** Standard settings windows under Edit.
**Facts.** Settings are the "Workspace and shortcuts" modal
(`workspace.rs:1027`, about 670 lines) with explicit Save and Close buttons.
**Do.** Preferences: a category list (General, Appearance, Viewport,
Shortcuts, External Tools, AI) and a page per category; changes apply at
once; Reset to Defaults per page; Shortcuts has search and conflict
warnings and lists every command. Project Settings: project name, startup
scene, Input Actions (moved from Edit), Build.
**Done when.** No Save button; everything from the old modal is present;
Ctrl+, opens Preferences.

#### S-11 · Viewport navigation and selection — M — ready
Status: done — Claude Opus 5.5 (UX helper), 2026-11-17
**Outcome.** The viewport handles like Unity's Scene view.
**Facts.** Navigation is in `crates/rot-render/src/lib.rs:1192-1235`:
right-drag orbits, middle-drag pans, wheel zooms, right button plus WASD
flies. There is no box select and no stats display. Renderer timings exist
in a `Metrics` struct (`rot-render/src/lib.rs:137`).
**Do.** Implement the scheme in [Scene and Game](UX-STANDARDS.md#6-panels).
Add box select, double-click to frame, an orientation gizmo in the top-right
corner, scroll-to-change-speed while flying, and a Stats overlay (frame
time, fps, draw calls, triangles) toggled from the viewport.
**Done when.** A Unity user's navigation habits work without reading
anything; Stats shows live numbers.

#### S-12 · Assets browser — M — ready
Status: done — Claude Opus 5.5 (AI lane helper), 2026-11-17
**Outcome.** Assets are a grid of thumbnails you drag from, not a list of
rows with buttons.
**Facts.** The panel is `browser_contents` (`scene_editor.rs:4248`). Import
is a form asking for an asset ID (`:4657`). File choosing shells out to
`zenity` (`scene_editor/project_picker.rs:429`).
**Do.** Grid and list views with thumbnails (texture previews first; a type
icon for the rest), search and a size slider. Selecting an asset shows it in
the Inspector. Replace per-row buttons with drag and drop (texture onto a
material slot, mesh onto an object, model into the scene) and a context
menu. Dropping files from the OS file manager copies them into the project
and imports them with an ID taken from the file name. Use the `rfd` crate
for native file dialogs everywhere.
**Done when.** Drop a `.glb` from the file manager onto the panel, then drag
it into the scene, with no form.
**Not now.** Folder tree, automatic reimport and sidecar files (C-04).

#### S-13 · Project hub and templates — S — ready
Status: done — 2026-11-17, Claude Opus 5.5 (UX helper). The list shows Unity Hub's "Modified" (scene file time), not a stored last-opened time (agent choice).
**Outcome.** A first launch that looks finished.
**Facts.** `scene_editor/project_picker.rs:30-135`; New Project creates one
empty scene; no templates.
**Do.** Hub with recent projects (name, path, last opened), New Project and
Open Project. New Project offers Empty and Basic 3D (ground, light, cube,
camera). First launch shows the style chooser from T-01 and an optional
"Connect an AI App" step from A-04.
**Done when.** Screenshots pass the checklist in both styles.

#### T-02 · Rot motif and themed wording — S — ready
Status: done — 2026-11-18, see git log; screenshots sent to the user for review
**Outcome.** The Rot style has its character, lightly.
**Do.** Follow [the motif](UX-STANDARDS.md#the-motif): empty-state line art,
the three-spore loading indicator, the AI mushroom pulse, hub and About
backgrounds, and the Rot column of the wording table. Depends on T-01 and
S-08.
**Done when.** At most one themed line per screen; Plain shows none of it;
the user has seen screenshots.

---

### AI lane

Target experience: the creator opens the editor, clicks Connect once for
their AI app, and from then on the AI can see and edit the open project
whenever both are running, with no prompts, tokens or environment
variables. See [A10](PARITY-PLAN.md#4-architecture-decisions).

#### A-01 · The editor is always reachable — M — ready
Status: done — 2026-11-17, 2c5811d
**Outcome.** `rot-mcp` started with no configuration finds the running
editor.
**Facts.** The editor listens only after Connect AI or with
`ROT_MCP_ENABLE=1` (`scene_editor.rs:6486`, `:6566-6575`). Each Connect
creates a new random directory under `$TMPDIR`
(`crates/rot-editor/src/bridge.rs:579-598`). The adapter needs
`ROT_MCP_PROJECT_REGISTRY` or `ROT_MCP_LOCATOR`
(`crates/rot-mcp/src/main.rs:861-868`). An editor the user opened by hand is
invisible to the adapter, which then starts a second editor on the same
file (`crates/rot-mcp/src/project_tools.rs:177-264`).
**Do.** While a scene is open the editor listens on
`$XDG_RUNTIME_DIR/rot/<pid>.sock` and writes `<pid>.json` beside it (pid,
project folder, scene, start time), removing it on exit. `rot-mcp` lists
those files, drops dead ones, and picks the editor whose project folder
contains its working directory, else the only one running. With several to
choose from, the existing `project_list` and `project_connect` tools list
and select running editors; with none running, tools answer "Rot Engine is
not running" and `project_connect` with a project path starts it. Do not
add new tool names. Put socket and process code in a small per-OS module.
Store an "allow AI apps to connect" setting in the preferences file,
default on; its checkbox arrives with S-10. **Keep the existing approval
step exactly as it is.** This package changes how the editor is found, not
who is trusted; trust changes in A-02.
**Done when.** Start the editor normally, run `rot-mcp` in a terminal, and
`initialize`, `tools/list` and a read tool work with no environment
variables; a headless test covers discovery with two editors and a dead
entry.

#### A-02 · Trust once — M — ready
Status: done — 2026-11-17, c9c6013 (reviewed; trust trade accepted by the user)
**Outcome.** An AI app is approved once and never again, across restarts and
rebuilds.
**Facts.** Unenrolled adapters need approval after every restart
(`crates/support/connection.rs:354`). Enrollment pins the inode and
modification time of both binaries, so any rebuild breaks it
(`crates/support/project_startup.rs:11-37`,
`crates/support/enrollment.rs:140-152`), and is created only by two
scripts. `scene_save` needs `ROT_MCP_SAVE_DIR`
(`scene_editor/persistence_commands.rs:22-29`).
**Do.** Keep a per-user store of trusted AI apps (name, random token,
allowed flag) in the config directory with owner-only permissions.
`rot-mcp --client <name>` presents the token. A known token connects
silently. An unknown client produces one non-blocking prompt: Allow Always,
Allow Once, Deny. Remove binary pinning. A trusted app may save the open
scene and write inside the project folder; builds go to `<project>/Builds`.
Revoke lives in Preferences → AI (until S-10, in the existing connection
panel). Write a short decision note: this replaces the session-bound
approvals of decision 0050. Because this changes the AI trust boundary, have
the strongest available model review the diff before merging.
**Done when.** Approve once; restart the editor, rebuild both binaries,
reconnect: no prompt. Revoke blocks the next call. Save works for a trusted
app with no environment variable.

#### A-03 · Smooth shared editing — M — ready
Status: done — Claude Opus 5.5 (AI lane helper), 2026-11-17
**Outcome.** The AI and the creator edit the same scene without tripping
over each other.
**Facts.** Every edit needs `expected_session_token` and
`expected_revision`; any human edit in between fails the AI's call with
`stale_revision` (`scene_editor.rs:3107`). A pending human draft or drag
rejects AI edits (`:1993-2018`). Any transport error retires the client for
good (`crates/support/connection.rs:368-380`). The queue holds one request
(`bridge.rs:690`). Integers must be decimal strings.
**Do.** Make the revision arguments optional: enforced when sent, otherwise
the edit applies to the current scene. Commit a valid pending draft before
applying an AI edit; during a drag, wait up to a second. Queue up to 32
requests and serve several per frame within a small time budget. Reconnect
on the next call after an error. Accept JSON numbers. Label AI steps in the
Undo history ("AI: Move Cube").
**Done when.** A scripted run where the human edits between the AI's read
and write succeeds; fifty edits in a row finish in under two seconds; after
killing and restarting the adapter the next call works.

#### A-04 · Connect an AI App — M — ready
Status: done — Claude Opus 5.5 (AI lane helper), 2026-11-17
**Outcome.** One click registers Rot Engine with the creator's AI app.
**Facts.** Nothing in the repository writes any AI app's configuration. The
panel says "Automatic Hermes registration is unavailable"
(`scene_editor/connection/native.rs:246`).
**Do.** A dialog listing AI apps found on the machine with a Connect button
each. First read each app's current official documentation for registering
a local MCP server; do not rely on memory. Start with Claude Code, Claude
Desktop, Codex, ChatGPT desktop, Cursor and VS Code, plus Copy Configuration
for anything else. Writing a config file means: read it, add or replace only
the `rot` entry, write atomically, keep a backup. Offer Undo. Also offer
"Add AI configuration to this project" for apps that read project-level
files.
Develop and test against copies of the config files. Ask the user before
the first write to their real configuration.
**Done when.** On this machine, with the user's go-ahead, Claude Code and
Codex connect through the button and run a tool with no manual editing;
other entries in their config files are unchanged; Undo removes the entry.
**Not now.** Provenance checks, custom transactions or hardening beyond an
atomic merge. An earlier attempt at this spent days there and shipped
nothing.

#### A-05 · AI indicator and activity — M — ready
Status: done — Claude Opus 5.5 (AI lane helper), 2026-11-17
**Outcome.** The creator can see what the AI is doing and stop it.
**Facts.** The mushroom badge only has a hover label
(`crates/rot-editor/src/branding.rs:60-77`); the panel is under File →
Advanced and shows internal terms.
**Do.** Clicking the indicator opens a popover: connected apps, a Pause AI
Edits switch, the recent AI actions with Undo on the latest, Connect an AI
App…, Disconnect. Grey when idle, Rot red when connected, slow pulse while a
request is running. Toast on connect and disconnect. Move diagnostics to
Preferences → AI and remove internal wording.
**Done when.** Screenshots pass the checklist; pausing makes AI edits wait
and resume.

#### A-06 · Slim the tool list — M — ready
Status: done — Claude Opus 5.5 (AI lane helper), 2026-11-17
**Outcome.** The tool list costs an AI client a fraction of today's context.
**Facts.** 98 tools, roughly 90–120 KB. Every tool repeats a `connection`
parameter and a long suffix (`crates/rot-mcp/src/main.rs:623-635`). The
server instructions are about 1.9 KB (`:704`). Five legacy logistics tools
are always listed. Screenshots need start-then-poll. The README and the
`scripts/mcp_*demo.py` files describe variables the code no longer reads.
**Do.** Drop the `connection` parameter and suffix (A-01 makes the target
implicit). Remove the legacy tools from the scene editor. Rewrite each
description in one or two plain sentences using the
[vocabulary](UX-STANDARDS.md#2-vocabulary). Cut the instructions to a short
paragraph. Add a single-call `viewport_screenshot`. Add a test that fails
if `tools/list` exceeds 40 KB. Fix the README; delete the stale demos.
**Done when.** `tools/list` is under 40 KB and a real client still completes
the A-07 journey.
**Not now.** The general object and component tools (C-06).

#### A-07 · Real-client check — S — ready
Status: done — Claude Opus 5.5 (AI lane helper), 2026-11-17
**Outcome.** One command proves the AI route works end to end.
**Do.** `scripts/mcp_smoke.py`, standard library only: start the editor with
`ui_drive.Editor`, start `rot-mcp` over stdio, then initialize, list tools
(print the size), read the scene, create and move an object, take a
screenshot, undo. Then, in a throwaway project with a project-level MCP
config, ask Claude Code or Codex to "add three cubes in a row and color them
differently", and note the result in STATUS.
**Done when.** The script passes and is listed in the PLAYBOOK's
verification table.

---

### Tools lane

#### P-01 · Baseline performance numbers — S — ready
Status: done — 2026-11-17, f8d6de6
**Outcome.** Known starting numbers for the
[budgets](PARITY-PLAN.md#5-performance-budgets).
**Facts.** `rot-render` has a benchmark binary (`--objects 3|256 --frames
N`). The scene cap is 256 objects until C-02.
**Do.** Measure in release: cold start to the hub, opening a small project,
editor memory, idle repaint count, renderer frame time at 256 objects, and
an MCP edit round trip. Write the numbers beside the budgets. Add larger
benchmark scenes after C-02.
**Done when.** Each Phase 1 budget has a measured value or a note on why it
could not be measured.

#### P-02 · One-command local install — S — ready
Status: done — 2026-11-17, e6a1ee8
**Outcome.** The user can try each finished package.
**Do.** `scripts/install_local.py`: build release `rot-editor` and
`rot-mcp`, install them with the desktop entry and icon under `~/.local`,
keep the previous version for `--rollback`. Ask the user before the first
run, because it replaces the installed pre-rename build.
**Done when.** Install, launch from the desktop launcher, roll back.

## Next — Phase 2: Core

The user authorized starting Phase 2 on 2026-11-19 after the independent Phase 1 checks. The human walkthrough and shared editing exercise ran on 2026-11-19. The full UI checklist remains open. C-01 comes first and gates the rest.

| Package | Outcome | Brief |
| --- | --- | --- |
| C-01 Object and component model | A written design for [A1 and A2](PARITY-PLAN.md#4-architecture-decisions): types, file format, Undo outside files, upgrade of the repository's example scenes, and the order of the port. | design drafted 2026-11-19 · [C-01 brief](plans/C-01-OBJECT-COMPONENT-MODEL.md) |
| C-02 Scene crate on the new model | `rot-scene` implements it. No object, depth or file-size caps. | in progress · the hub, File → Open and native `--scene` open v23 for basic Inspector edits, object creation, parenting, deletion, Undo/Redo and Save. Scene switching protects v23 drafts. Automatic upgrade and tracked example conversion are open |
| C-03 Renderer, runtime and scripts on the new model | Everything that reads a scene uses objects and components. | in progress · editor Play, standalone viewer, headless behavior and packaged Motion Lab support v23 through previews and runtime projections. Native editor and viewer use world matrices for draw, fit and hit testing. General component execution remains open; v23 can build a Linux player folder |
| C-04 Assets as files | `Assets/` folder, sidecar ids and import settings, rebuildable cache, file watcher, automatic import and reimport, folder tree in the browser. | brief needed |
| C-05 Hierarchy and Inspector | Rebuilt on components: Add Component, drag to reparent, multi-edit, enable toggles, visibility and lock, live values during Play. | brief needed |
| C-06 General MCP tools | A small set of object, component, asset, play and screenshot tools that call the command layer. Tool list under 15 KB. | brief needed |
| C-07 Prefabs | Prefab assets, instances, per-property overrides, apply and revert. | brief needed |
| C-08 General glTF import | Multi-primitive meshes, full PBR material mapping, cameras and lights; warnings instead of refusals. | ready after C-04 |
| C-09 Build and Run | The editor ships a player binary; Build writes a runnable folder; no Python. | in progress · v23 editor builds and launches a Linux player folder. Legacy Build still uses the runtime kit |
| C-10 Scripts as files | Lua files in `Assets/`, one growing API (lifecycle, find, spawn, destroy, components, input, time), reload on save, open in the creator's code editor. | brief needed |
| C-11 Scenes and projects | Several scenes per project, a project settings file, a startup scene. | ready after C-02 |
| C-12 Benchmark scenes | 5,000- and 50,000-object scenes and a one-command frame-time report. | ready after C-02 |

## Later

One line each until their phase is next. See the
[phases](PARITY-PLAN.md#3-phases).

- **Phase 3 — Look.** PBR material with normal, metallic, emissive and
  occlusion maps; material assets with a preview; transparency and
  double-sided; spot lights and many lights; cascaded, spot and point
  shadows; sky, image-based lighting and fog; bloom, color grading, ambient
  occlusion, anti-aliasing options; culling and instancing; shading and
  debug view modes.
- **Phase 4 — Play.** Events and coroutines in scripts; Rapier rigid bodies,
  colliders, layers, raycasts, triggers, joints and a character controller;
  skinned meshes, animation state machine and blending; looping, streamed
  and spatial audio with a mixer; runtime UI with anchors, text, images,
  buttons and layout; mouse and gamepad input with rebinding; several
  cameras and follow rigs; scene loading; particles; save data.
- **Phase 5 — Ship and scale.** Windows editor and builds; profiler; script
  debugger polish; LOD and occlusion culling; terrain; navigation;
  timeline; light probes or baking; editor extensions; localization.
- **Phase 6 — High end.** Ray tracing; virtualized geometry; streamed
  worlds; networking; more platforms.

## Done

- 2026-11-17 · Imported GLB transform animation (import, Place, Play,
  Refresh, Build). `5e24664`…`298428b`.
- 2026-11-17 · Renamed remaining Ruster identifiers to Rot. `9a4ed8f`.
- 2026-11-17 · `scripts/ui_drive.py`: drive and screenshot the editor on a
  hidden workspace.
