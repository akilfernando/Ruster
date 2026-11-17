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

## Inbox

One line each. Anyone may add; the user or a planning pass orders them.

- The legacy logistics prototype (`rot-core`, `rot-cli`, `rot-viewer`,
  `rot-script`, `examples/yard-app`, editor `ui.rs` and `model.rs`, five
  legacy MCP tools) is unused by the scene editor. Proposal: delete it.
  Needs the user's yes.
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
Status: open
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
Status: open
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
Status: open
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

#### S-04 · One toolbar; view options move into the viewport — M — ready
Status: open
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
Status: open
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

#### S-06 · Console, status bar and toasts — M — ready
Status: open
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
Status: open
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

#### S-07 · Docked, tabbed panels — L — brief needed
Status: open
**Outcome.** Hierarchy, Inspector, Assets, Console, Scene and Game are real
panels that dock, tab, close and reopen, with saved layouts.
**Facts.** Hierarchy and Inspector are rectangles painted over the viewport
(`scene_editor/sidebars.rs:96-158`). Assets is `Panel::bottom`
(`scene_editor.rs:5711`). No docking crate is used. The AI workspace has its
own drawers (`scene_editor/ai_workspace.rs:128`).
**Split.** (a) Pick a docking crate that works with the egui version in
`Cargo.toml` in under an hour; if none does, use fixed side panels with tab
strips. (b) Move each panel into `crates/rot-editor/src/panels/<name>.rs`
and into the dock. (c) Layouts: Default and Side by Side (for a 960 px wide
window next to an AI app), saved per user, with F6 to cycle panels; retire
the separate AI workspace drawers.
**Done when.** A tab can be dragged to another dock; panels close and reopen
from Window; the layout survives a restart; nothing is painted over the
viewport; the editor works at 960×1080.
**Not now.** Floating OS windows and multiple viewports.

#### S-08 · String table and wording pass — M — ready
Status: open
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
Status: open
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
Status: open
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
Status: open
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
Status: open
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
Status: open
**Outcome.** A first launch that looks finished.
**Facts.** `scene_editor/project_picker.rs:30-135`; New Project creates one
empty scene; no templates.
**Do.** Hub with recent projects (name, path, last opened), New Project and
Open Project. New Project offers Empty and Basic 3D (ground, light, cube,
camera). First launch shows the style chooser from T-01 and an optional
"Connect an AI App" step from A-04.
**Done when.** Screenshots pass the checklist in both styles.

#### T-02 · Rot motif and themed wording — S — ready
Status: open
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
Status: open
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
Status: open
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
Status: open
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
Status: open
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
Status: open
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
Status: open
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
Status: open
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
Status: open
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
Status: open
**Outcome.** The user can try each finished package.
**Do.** `scripts/install_local.py`: build release `rot-editor` and
`rot-mcp`, install them with the desktop entry and icon under `~/.local`,
keep the previous version for `--rollback`. Ask the user before the first
run, because it replaces the installed pre-rename build.
**Done when.** Install, launch from the desktop launcher, roll back.

## Next — Phase 2: Core

Start when Phase 1's exit test passes. C-01 comes first and gates the rest.

| Package | Outcome | Brief |
| --- | --- | --- |
| C-01 Object and component model | A written design for [A1 and A2](PARITY-PLAN.md#4-architecture-decisions): types, file format, Undo outside files, upgrade of the repository's example scenes, and the order of the port. | brief needed |
| C-02 Scene crate on the new model | `rot-scene` implements it. No object, depth or file-size caps. | after C-01 |
| C-03 Renderer, runtime and scripts on the new model | Everything that reads a scene uses objects and components. | after C-01 |
| C-04 Assets as files | `Assets/` folder, sidecar ids and import settings, rebuildable cache, file watcher, automatic import and reimport, folder tree in the browser. | brief needed |
| C-05 Hierarchy and Inspector | Rebuilt on components: Add Component, drag to reparent, multi-edit, enable toggles, visibility and lock, live values during Play. | brief needed |
| C-06 General MCP tools | A small set of object, component, asset, play and screenshot tools that call the command layer. Tool list under 15 KB. | brief needed |
| C-07 Prefabs | Prefab assets, instances, per-property overrides, apply and revert. | brief needed |
| C-08 General glTF import | Multi-primitive meshes, full PBR material mapping, cameras and lights; warnings instead of refusals. | ready after C-04 |
| C-09 Build and Run | The editor ships a player binary; Build writes a runnable folder; no Python. | ready |
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
