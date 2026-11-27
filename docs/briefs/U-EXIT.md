# U-EXIT · The editor passes its own standard, checked by a person

Lane UX · Size M · Depends on: U-01 to U-06

**Outcome.** Every editor surface passes the UI checklist, and the user has
done the walkthrough by hand on the current editor.

**Why.** The Phase 1 exit test is a person completing six tasks without
help. The user last did it on 2026-11-19, on the editor that was removed on
2026-11-26. Injected input is not a walkthrough.

**Who.** Part 1 is for the strongest available model. Part 2 is for the
user. An implementing agent does not mark this package done.

**Part 1: review.**

1. Build `rot-editor` with the `inspection` feature. Open
   `examples/rot-yard/gallery.scene.json` and the platformer sample.
2. Capture every surface at 1280×800, 1920×1080 and 960×1080 with
   `scripts/ui_drive.py`: main window, each menu, Add Component, each
   component's Inspector section, material editor, Scene Display, Assets with
   content, Console, Script Debugger, Preferences and Project Settings pages,
   Build dialog, project hub, AI Connection, Game view, Play Values.
3. Apply the [UI checklist](../UX-STANDARDS.md#10-ui-review-checklist) to
   each. Fix what takes under ten minutes. Write each larger problem as one
   Inbox line in [BACKLOG](../BACKLOG.md) with the screenshot name.
4. Run `tabwalk` on each dialog and panel.
5. Set this package to `review` in the backlog.

**Part 2: walkthrough by the user (about 20 minutes).** Use the six tasks in
[the editor comparison](../research/EDITOR-WORKFLOW-COMPARISON.md#proposed-editor-baseline-walkthrough)
on a disposable copy of the platformer sample:

1. Find an object, look at it from the top, return to perspective.
2. Rename it, type a new position, cancel a second edit with Escape.
3. Move a group with the gizmo and assign a material.
4. Play, see a deliberate script error, fix it, play again.
5. Connect an AI app, ask it to add three cubes while you move another object,
   then Undo through both.
6. Save, close, reopen, Build and Run.

**Done when.**

- Part 1 found no checklist failure that is not either fixed or in the Inbox.
- The user has done Part 2. Their remarks are quoted in the Inbox, one line
  each, and the result is recorded in the research file under a new dated
  heading, as the 2026-11-19 result was.
- Only then is the status `done`.
