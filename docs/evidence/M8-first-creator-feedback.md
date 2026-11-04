# First Unity-oriented creator feedback — 2026-11-04

The user reports mainly Unity experience and voluntarily tried the verified manipulation editor in a disposable three-cube scene on workspace2. This was a real creator interaction with no injected input during the walkthrough. Root gave an outcome-oriented task: create/name/duplicate, multi-object transforms, Undo, numeric editing and Save. It was a short qualitative check, not a timed usability study or a completed per-task acceptance matrix.

The user reported the experience was good overall and identified:

- Cubes looked noticeably aliased.
- Enter in the rename dialog did not submit; clicking Apply was required.
- Expected perspective/ISO scene views were missing or undiscoverable.
- Keyboard-comfortable creators need Enter submission, traversal, shortcuts and hotkeys throughout the workflow.

Code inspection confirmed the tested build had single-sample scene attachments/pipeline and perspective-only camera controls; the user had not missed an existing orthographic toggle. These findings became required M8 corrections rather than optional polish: keyboard-dialog audit, visible projection/axis navigation and antialiasing with measured quality/cost.

The window was opened silently on workspace2 with the existing routing rule; the user controlled its focus/workspace. Root later observed workspace1 active and the recorded walkthrough process absent. The original disposable destination still contained its initial format3/three-object document, so Save/task completion is **not inferred** from the positive overall comment. No individual task timings, physically focused gesture sequence, shortcut matrix or Unreal familiarity are established. Background build activity was paused for the walkthrough; this does not turn subjective feedback into a performance measurement.

The launch enabled the same explicit Vulkan validation setup as earlier native tests. The captured `/tmp/ruster-human-m8/native.log` passed `scripts/check_vulkan_log.py`; it showed loaded instance/device validation and adapter setup, with no retained frame/exit summary. This ephemeral log is supplemental, not a continuity dependency; [durable native tool evidence](M8-manipulation-tools-native.md) records the matching tested implementation and its diagnostic limits.

Follow-up implementation/verification belongs in the current STATUS and workspace/AA evidence. One mostly-Unity creator's feedback does not satisfy all M9 onboarding or experienced-Unity-and-Unreal gates.
