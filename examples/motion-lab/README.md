# Motion Lab

A small runnable motion-authoring prototype using `ruster-app`, its shared Vulkan viewport and the same fixed-step `Runtime` as Yard. Select procedural objects, attach a Rhai behavior, edit per-object numeric properties and inspect motion. This slice is not the completed M8 editor or an IDE integration.

```sh
cargo run --locked -p ruster-motion-lab
cargo run --locked -p ruster-motion-lab -- --headless --ticks 10
cargo run --locked -p ruster-motion-lab -- --headless --ticks 10 --speed 2
cargo run --locked -p ruster-motion-lab -- --script examples/motion-lab/resources/motion.rhai
```

The embedded default script requires no resource directory. Native mode starts paused with no behavior attached. Select `left` in the hierarchy or viewport, click **Attach behavior (draft)**, change `speed` or `amplitude`, then click the host's **Reset** and **Run** or **Step**. The inspector shows applied attachment state and runtime transforms. Run/Pause also uses Space when a text field does not own keyboard focus. `--speed N` preattaches `left` and applies the override in either mode; headless always attaches `left`.

The fixed tick is 0.1 seconds. With the original source, ten ticks at speed1 produce `left` position `[-2, 0.841471, 0]` and Y rotation1 radian; speed2 produces Y0.909297 and Y rotation2. Other objects stay at their authored transforms until attached. `--frames N` bounds native smoke runs; `--ticks N` bounds headless runs (maximum10000).

Source and attachment/property edits are drafts until **Reset**. Reset compiles the candidate, validates every attachment's property schema and trials its first update on a disposable scene. Only success applies the whole candidate and restores all authored transforms at tick0. Attachments may share code while keeping independent values. **Detach behavior (draft)** also takes effect at Reset. There is no application of play transforms back into authored content.

To edit externally, pass `--script PATH`, save the file in your editor and click **Reload from disk (draft)**, then **Reset**. Reads accept regular UTF-8 files of at most16KiB. Reload is explicit; the app never launches an editor or writes the source file. The built-in multiline source editor works without any external tool. All built-in edits, attachments and numeric values last only for this session; there is no project Save, undo history or reopen persistence in this slice.

For error recovery, first apply and run the original source. Replace the source draft with `fn properties() {` and click Reset: an error is visible and the last applied transforms/behavior remain. Click **Revert all drafts to last applied**, then Reset to recover. This discards draft source, attachment and numeric edits together; runtime transforms remain until Reset. For an external-file failure, make the same edit on disk and Reload; restore the file and Reload, then Reset. If you rename an exposed property, Reset rejects old property maps; click **Use source defaults for all draft attachments**, adjust values and Reset. This action deliberately replaces all draft attachment values.

Runtime errors can still occur after the first update. The host pauses with object/source context; the entire failed frame preserves its previous transforms and tick. Correct the draft or detach the offending object, then Reset. A script that fails during headless execution prints the error and exits2. Syntax-invalid startup files also exit2 with a recovery hint; start with the embedded script to repair through the UI.

The script defines `properties()` returning numeric defaults and `update(object, props, dt, time)` returning the complete transform map. Keys are `x,y,z`, `rx,ry,rz` (radians), `sx,sy,sz`; `dt` and `time` are seconds. Preserve all transform keys. The script adapter enforces bounded evaluation and validated finite transforms; this is a trusted local authoring workflow, not hostile-code isolation. See the [script crate](../../crates/ruster-script/README.md) for the supported language/bounds.

The focused integration checks use the real host Runtime to protect draft/application separation, invalid disk reload and schema migration, first-update candidate rejection, whole-frame atomicity across multiple objects, pause on later runtime error and Reset recovery. They supplement the script crate's language/limit tests without duplicating that matrix:

```sh
cargo test --locked -p ruster-motion-lab
```

Native tests must follow the project's workspace2 isolation procedure in `AGENTS.md`. Human Unity/Unreal familiarity, generic editor controls, project persistence, IDE launch and full M8 acceptance remain unfinished.
