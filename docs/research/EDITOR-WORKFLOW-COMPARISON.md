# Editor workflow comparison

Research checkpoint 2026-11-16. Compared the official Unity 6, Unreal Engine 5
and Godot stable editor documentation against Rusterize cumulative `5e83590`,
its owning UI code and retained user feedback. The documents establish interaction
patterns and defaults. The proposed benefits below are hypotheses until creators
exercise matching tasks. No new engine sessions or physical walkthroughs were
run. Existing [editor requirements](../EDITOR-EXPERIENCE.md) and
[design direction](../EDITOR-DESIGN.md) govern.

## Documented patterns

| Task | Unity | Unreal | Godot | Common expectation |
| --- | --- | --- | --- | --- |
| Find an object and edit it | Hierarchy/Scene selection and Inspector, plus Project window for assets | Outliner/Level Viewport selection and Details, with Content Browser/Drawer | Scene tree and workspace selection, Inspector and FileSystem | Selection has one intelligible identity across view, tree and inspector. Assets and placed instances need visible distinction. Placement of panels varies. |
| Work precisely in a scene | Scene Camera orientation overlay, orbit/pan/zoom, frame selection, gizmos and snapping | Viewport toolbar for tools/snapping/projection and navigation schemes, with frame selection | 3D viewport and orientation controls, transform tools and configurable shortcuts | Visible tool and axis state, fast navigation recovery, direct manipulation and precise inputs are expected. Defaults and gesture mappings vary. |
| Inspect or run a game | Scene and Game views with Play controls, Console | Level Viewport with Play/Simulate and explicit runtime controls, output and diagnostics | Run project/scene, editor workspaces and bottom docks for Output/Debugger | A distinct runtime context and an easy route from failure to investigation are common. Editor camera and player view have different purposes. |
| Work with less chrome | Configurable windows/layouts and Scene overlays | Temporary Content Drawer plus a persistent Content Browser option | Task workspaces and a distraction-free view | A temporary panel can preserve screen area while keeping familiar controls reachable. Full docking is one possible implementation. |
| Transfer habits | Shortcut management and discoverable Scene tools | Configurable editor bindings, context actions, documented viewport mappings | Editor Settings shortcuts and documented default map | Familiar vocabulary and visible key hints support transfer. No single complete keymap is common to all three. |

Primary documentation accessed 2026-11-16:
[Unity Editor windows](https://docs.unity.com/en-us/engine/6000.6/manual/unity-editor),
[Unity Scene navigation](https://docs.unity.com/en-us/engine/6000.0/manual/unity-editor/editor-windows-views-reference/using-the-scene-view/scene-view-navigation),
[Unreal Editor Interface](https://dev.epicgames.com/documentation/en-us/unreal-engine/unreal-editor-interface),
[Unreal Viewport Controls](https://dev.epicgames.com/documentation/en-us/unreal-engine/viewport-controls-in-unreal-engine),
[Unreal Outliner](https://dev.epicgames.com/documentation/unreal-engine/outliner-in-unreal-engine),
[Godot first look](https://docs.godotengine.org/en/stable/getting_started/introduction/first_look_at_the_editor.html),
[Godot feature inventory](https://docs.godotengine.org/en/stable/about/list_of_features.html).

Three differences are particularly relevant. Unity and Unreal both document
right-mouse fly navigation, but map fly-up and fly-down to opposite Q/E keys.
A remappable, discoverable convention is a better hypothesis than merging
the two keymaps. Unity documents temporary Play changes, Unreal offers
explicit Keep Simulation Changes, and Godot distinguishes Local and Remote
editing. Rusterize therefore needs to communicate the specific state a field
is editing. Godot's Inspector documents numeric dragging, property search,
per-property revert indicators and compatible resource drop feedback. These
provide concrete ways to make properties easier to work with while keeping
the viewport free of extra copy.
[Unity shortcuts](https://docs.unity3d.com/6000.0/Documentation/Manual/ShortcutsManager.html)
[Unity Game view](https://docs.unity3d.com/6000.0/Documentation/Manual/GameView.html)
[Unreal Play and Simulate](https://dev.epicgames.com/documentation/unreal-engine/ineditor-testing-play-and-simulate-in-unreal-engine?lang=en-US)
[Godot game embedding](https://docs.godotengine.org/en/stable/tutorials/editor/game_embedding.html)
[Godot Inspector](https://docs.godotengine.org/en/stable/tutorials/editor/inspector_dock.html)
[Godot layout customization](https://docs.godotengine.org/en/stable/tutorials/editor/customizing_editor.html)

## Rusterize implications to test

- **Selection is the first task.** In a scene with similar objects, picking in
  the viewport should identify and reveal the same object in the tree and
  immediately show editable values. A tree search should lead to Frame quickly.
  Keep selection and draft state legible when changing between Full editor and
  AI workspace. The user previously confirmed two specific repairs, reliable
  picking and live Transform feedback. Repeat the broader task on a current
  ordinary build before drawing wider conclusions.
- **Preserve keyboard flow.** Put a visible route and matching key hint on
  common actions. Test Tab and Shift+Tab, Enter, Escape, remapping, and shortcut
  collisions in names, numbers, search, modal dialogs and the viewport. Show
  validation beside the owning field and retain recoverable text. A successful
  commit should enter Undo once. Include explicit multi-object mixed-value and
  focused-field cases so a single-value demo cannot hide destructive edits.
- **Make the viewport explain its current operation.** Keep projection/axis,
  tool, coordinate space, pivot and snapping discoverable without requiring
  persistent labels for every option. Link gizmo movement to the inspector and
  show selection with more than a color cue. Verify legibility and AA in both
  wide and compact modes at recorded OS scale. Frame changes the editor view. Moving the object or changing its player
  camera changes authored state.
- **Keep authoring and Play state visible.** A creator should know which view
  they are using, where input is captured, which values are transient and how to
  return to authoring. Show script or packaging errors with the next useful
  action and retain enough context to resume. Test failure recovery during a
  normal edit-run-fix-save loop.
- **Use the compact workspace deliberately.** Let Objects and Selection open
  quickly and return to the viewport. Preserve Full editor layout, selection,
  drafts and scene/history across presentation changes. Test side-by-side with
  ChatGPT and with keyboard alone. Measure recovered viewport space and any
  added panel-open steps before considering a general docking implementation.
- **Teach through the product and docs.** Put controls and names in predictable
  places. Use a first-run guide that explains the equivalent jobs in Unity,
  Unreal and Godot terms, a compact shortcut reference, and task documentation
  for recovery, import and build errors. Verify instructions with someone who
  has not seen the fixture or authoring notes.

Source sampling on `5e83590` covered `scene_editor.rs` hierarchy, Inspector,
compact layout and shortcut dispatch, `workspace.rs` bindings, and the
`scene_editor/ai_workspace.rs` drawer controls. Existing component checks
exercise layout restoration, search and shortcut focus conflicts. The November
16 actual-client trial establishes bounded native/MCP behavior. Physical
keyboard/mouse comfort, broader first-run familiarity and ordinary installed
launch with current preferences remain open. The historical user walkthrough
and existing evidence distinguish user-observed behavior from injected events.
[User feedback and verification boundaries](../EDITOR-EXPERIENCE.md)
[Cumulative editor status](../STATUS.md)

## Proposed editor baseline walkthrough

Use one exact installed or source-pinned build and a disposable mechanism scene.
Record build, platform, window geometry, scaling, input device, and prior engine
experience. Run with Unity- and Unreal-experienced creators when available, with
Godot users useful as another reference. Start with a wide window and repeat
layout-sensitive tasks in a narrow side-by-side configuration. Record unassisted
success, time, mistaken actions, prompts requested, recovery path and participant
notes. Use observed baseline data to set improvement targets. Preserve failed
attempts, and record whether evidence is human operation or injected input.

| Task prompt | Observable exit |
| --- | --- |
| Find the hinge and look at it from the top, then return to perspective | Participant finds the intended object without relying on its color, frames it, identifies current projection and returns without altering the scene. |
| Rename it and change a transform numerically, then cancel a second edit | Enter commits, Tab traverses, Escape cancels the intended draft, and the viewport, inspector and Undo agree. |
| Move a group with a gizmo and assign an existing material | Tool, snap and pivot are legible, live fields match the edit, references survive save/reopen, and Undo/Redo affect the expected transactions. |
| Run, observe a deliberate error, fix it and run again | Participant distinguishes Play state and input capture, finds a useful error, resumes editing and reruns successfully. Mark unsupported fixture behaviors explicitly. |
| Connect an AI app and verify an edit while arranging the scene | Ask the AI app to add three cubes in a row with different colors while you move a different object. Identify the AI edits in Scene and Inspector. Check that Undo follows the order of your edits and the AI edits. |
| Save, close and reopen, then locate the export result or resolve a deliberate missing asset | Participant can recover without assistance, saved data is exact and the chosen build path identifies what shipped or why it refused. |

Use the same task definitions for a baseline and any candidate. This is a
proposed research follow-through and can accompany the next selected engine
slice. No UI redesign, shortcuts preset or docking library is selected here.

### Human walkthrough — 2026-11-19

The user operated the local editor from the 5cc7181 test session with
disposable copies of the arrangement and Motion Lab scenes. Results are
participant reports.

| Task | Observed result |
| --- | --- |
| Find and frame | Found the object, switched to Top and returned to Perspective without help. |
| Keyboard editing | Rename, typed transform commit, cancellation and Undo worked. The user asked whether typing should preview before Enter. |
| Groups and materials | Required help. New Group created an empty group and dragging rows did not reparent them. The Inspector parent dropdown worked. Material assignment succeeded but the user expected later material edits to take effect before Apply Material. |
| Play and recover | Play and Stop were clear from the red border. Pause was easy to miss because only the window header changed. The scripted failure was corrected and the scene ran again. The user asked for clickable error details and source navigation and reported that expanding or collapsing Last failure details dismissed Game Controls. |
| Shared editing | ChatGPT and Rot both asked to connect, and ChatGPT requested several approvals during edits. Undo reversed later AI operations, then the user's move, then earlier AI operations. |
| Save and reopen | Group, position and material survived reopening. Export and missing-asset recovery were outside this run. |
