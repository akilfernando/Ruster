# Imported transform animation — 2026-11-17

Qualification of the [goal plan](../plans/IMPORTED-TRANSFORM-ANIMATION.md)
and [ownership decision](../decisions/2026-11-17-imported-transform-animation.md)
on `claude/rot-next-20261117`. Raw artifacts are in
`/home/akilf/Projects/Ruster-Audits/imported-animation-20261117/`.
Native and relocated runs used source `298428b`.

## What passed

- **Automated gates.** `python3 scripts/check.py --full` passed in 64.80 s
  on the integrated tree: formatting, strict workspace Clippy, workspace
  tests, script tests and Markdown. Inspection-feature editor Clippy
  passed in the editor worker's run.
- **Real Blender source.** Blender 5.2.3 exported two revisions of a
  three-node textured beacon whose cap has one action, `CapSpin`
  (`export_animated_fixture.py`). Blender samples every frame: 49 LINEAR
  keys per track from 1/24 s to 49/24 s. The ignored asset test
  `blender_sampled_clip_imports_and_refreshes` imported A, read both
  tracks, refreshed to B and kept the clip ID.
- **Ordinary editor journey (`native-2`).** The inspection editor
  (SHA-256 `cde00a66…2c57`) ran on hidden workspace 2 on the RTX 4060
  with app-scoped events. Import Asset → Browse (fixture chooser) →
  Import asset published catalog v7 and the row
  `beacon · Animated model · CapSpin (2.04 s)`. Place showed the
  Animation section. Reset, Play, Pause and Stop ran the preview. File →
  Save wrote Scene22 with one looping binding mapping three nodes.
  Overwriting the source with revision B and Refresh adopted the new
  generation with identical scene bytes. File → Export application
  published an archive through a kit prepared from this source, and
  File → Close window closed normally. Viewed captures: at Reset the cap
  sits at its rest pose; paused at tick 71 revision A shows it raised and
  turned; revision B at tick 71 shows about twice the lift.
- **Relocated application.** The archive (SHA-256 `573a3320…92eb`) was
  extracted to another directory and run from `/tmp` with
  `--headless --ticks 0`, `36` and `71`, each exiting 0. It carries
  descriptor 7 and `resources/animations/0000.json`. Cap translation at
  each tick equals an independent linear interpolation of the shipped
  clip keys within 5e-8. Authored transforms in the same reports stay at
  the rest pose.
- **Inspector controls (`native-4`).** Pointer activation of **Loop** and
  **Remove animation** committed, and Edit → Undo restored the binding.

## Limits and retained failures

- **Keyboard reach of the Inspector is not qualified.** Tab traversal
  never enters the Objects or Inspector overlay panels: after **Fit**
  focus drops to the window and wraps to **File**
  (`current-tab-tab-order.json`). The 2026-11-16 inspection build shows
  the same order (`baseline-tab-tab-order.json`), so this predates this
  slice. Cause is undiagnosed after one bounded attempt. It blocks
  keyboard use of every Inspector control, including the new Animation
  section. Runs `native-3` and `native-5` are retained harness attempts
  from this investigation, not passes.
- **Guarded MCP.** `scene_animation_set` passed through the real editor
  command route and adapter argument validation in tests. No STDIO
  adapter or actual AI client run was performed.
- **Play checks** still refuse scenes with a hierarchy, so a saved check
  cannot cover an animated placement yet.
- Native logs pass `scripts/check_vulkan_log.py`. No validation layer was
  activated, so this is not a fresh validation claim. Hidden-workspace
  rendering may be throttled; no performance figure is claimed. No cost
  measurement of import, Step or memory was made.
- Events were injected app-scoped. This is not a physical human
  walkthrough. Nothing was installed.
- The first full gate after the rename failed on untracked private
  attachments in this checkout; they are now excluded locally through
  `.git/info/exclude` and were not opened.
