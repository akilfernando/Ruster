# Editor familiarity and everyday usability

Updated 2026-11-04. **Confirmed user requirement:** an experienced Unity or Unreal creator should arrive in Ruster and feel at home immediately. The small quality-of-life behaviors people rely on every day are non-negotiable product requirements. They must be designed into workflows and verified, rather than left as optional polish after the systems work.

This requirement strengthens the [creator ethos](DIRECTION.md#project-ethos-serve-the-creator). It does not assert that the current prototype already meets it. Historical M0–M6 prototype acceptance remains historical; it cannot substitute for editor experience acceptance.

## What the requirement means in practice

The following is the working breakdown of that requirement, selected by the agent for implementation planning. It is not an exhaustive list or a claim that the user specified each control. Add overlooked everyday friction when observed; do not use this list as a ceiling.

| Workflow | Expected baseline to design and verify |
| --- | --- |
| Orient and navigate | Recognizable scene hierarchy, viewport, inspector and content browser; obvious selection; predictable orbit/pan/zoom and fly navigation, frame selection, adjustable movement speed and useful camera reset. |
| Manipulate objects | Create, name, rename, duplicate, copy/paste and delete; multi-selection; move/rotate/scale gizmos; local/world modes, pivot choices and snapping; clear feedback about the active tool and affected objects. |
| Edit properties | Consistent numeric fields with precision entry and scrubbing, sensible increments, reset/default actions, multi-object editing with mixed-value states, predictable commit/cancel and text selection. |
| Find and organize | Search/filter objects and assets, contextual actions, drag-and-drop assignment and organization, clear reference navigation, recent projects/scenes and remembered working context. |
| Work by muscle memory | Discoverable shortcuts shown in menus/tooltips, remappable bindings, consistent selection modifiers and contextual commands. Text editing, viewport navigation and global shortcuts respect focus; Escape cancels the current interaction predictably. |
| Experiment without losing work | Consistent undo/redo across supported authored edits, meaningful action labels, one sensible undo step per drag, visible dirty state, safe save/close, autosave/recovery and explicit behavior for operations outside undo. |
| Iterate and diagnose | Clear play/pause/step/stop state, visible authored/runtime boundaries, responsive feedback, progress and cancellation for long work where applicable, actionable errors with source/context and a path back to the task. |
| Script behavior | Select an object, create/attach a behavior, open a configured code editor, discover the API, expose properties/references in the inspector, save/run quickly, navigate errors to source and inspect execution with a debugger. Script/property edits participate in project save/reopen and undo. |
| Make the workspace comfortable | Resizable/rearrangeable panels and persisted layouts, readable scaling, keyboard traversal, visible focus, adequate contrast and cues beyond color, sensible defaults and ways to restore them. |

Current agent-selected shortcut, navigation and workspace defaults are recorded in [decision0015](decisions/0015-keyboard-workspace-and-camera.md) and the editor guide. They remain subject to creator feedback; bounded local recovery is implemented and native-verified; full platform/human acceptance remains separate. Unity and Unreal users may bring different habits: use primary documentation and representative observations to revise defaults. Familiarity must be demonstrated in interaction, not inferred from a similar-looking screenshot. This requirement does not itself promise project-file compatibility, asset-format parity or the complete feature catalog of either engine.

## Delivery and acceptance

- Include the relevant everyday interactions in each workflow's definition of done. A transform feature includes selection, manipulation, precision, cancellation, focus and undo behavior; an asset feature includes finding, assigning and recovering content.
- Keep a visible gap list as features are introduced. Internal prototype slices can remain bounded, but missing baseline behavior must be identified as unfinished product work rather than silently waived because a lower-level system passes tests.
- Before claiming the editor is ready for the intended audience, require observed walkthroughs from experienced Unity users and experienced Unreal users. Ask them to create a small scene, find/import/assign content, manipulate and duplicate objects, change properties, undo mistakes, run/stop, save/reopen and recover an error using the product's own help. Record where familiar actions fail, where coaching is needed and where work is lost or interrupted.
- Fix blockers in those supported workflows before editor readiness/public-alpha acceptance. Agree concrete task thresholds from the first baseline; no arbitrary usability score or timing guarantee is asserted now. Automated native interaction protects behavior, but cannot establish that people feel at home.
- Apply the existing [testing policy](TESTING.md): focused behavioral checks and a few real journeys, with human evidence for familiarity. Do not duplicate every action at every test layer.

## Current baseline and gaps

At M6, the inspected generic scene editor provides a hierarchy, viewport selection, transform text fields, mesh assignment, bounded scene undo/redo, save/open, dirty/draft guards, orbit/zoom and Fit. Existing native and MCP verification proves the documented narrow journeys. External human familiarity remains untested.

Those M6 gaps drove the construction/manipulation updates below. Current construction, grouped history and tools are implemented; workspace/keyboard/camera refinements have integrated native evidence. Valid numeric drafts now commit when selection/focus changes, while invalid edits are preserved for repair. Asset catalog operations still persist separately from scene undo. Discovery/context/internal drops, bounded recovery and generic behavior editing/runtime are implemented with native evidence. Final debugger/MCP and regenerated LuaLS acceptance passed; see the M8 completion evaluation.

Implementation evidence: `crates/ruster-editor/src/scene_editor.rs`, `workspace.rs`, the renderer camera API, editor guide and linked native evidence. The first actual creator feedback is recorded below; automated checks alone do not establish familiarity.

## Relationship to the roadmap

Keep M7 standalone application kit, M8 independent reuse and M9 public alpha as the current sequence. This requirement raises the editor experience acceptance bar; it is not satisfied merely by completing those systems milestones. Relevant work belongs alongside each affected workflow, and remaining baseline gaps must be explicitly scheduled before editor/public-alpha acceptance. A dedicated editor completion slice may be needed after a scoped gap assessment; no extra milestone or all-at-once rewrite is selected here. The user requirement takes precedence over older wording that treats everyday usability as optional polish.


## Implementation allocation

2026-11-04, agent-selected working slices under authorization to proceed with M7. This allocates the confirmed requirement; it does not mark any gap complete or promise exact control mappings. M7/M8/M9 order is unchanged.

| Slice | Concrete scope and acceptance |
| --- | --- |
| M7 application kit, current | Clear Run/Pause/Step/Reset, discoverable controls, keyboard focus isolation, bounded timing after suspend, visible application outcomes and errors, executable-relative resources and a documented edit/build/package loop. Verify native input and relocated package; keep editor readiness unclaimed. |
| M8 authoring foundation, before reuse acceptance | Generic object create/rename/duplicate/delete and copy/paste; multi-selection and coherent undo; transform gizmos, snapping, local/world/pivot choices and precision fields. First assess existing toolkit support and current primary Unity/Unreal interaction references, then test the real authoring journey. |
| M8 content and workspace, before reuse acceptance | Search/filter and contextual actions, drag/drop reference assignment, useful content organization, frame selection/navigation controls, remappable/discoverable shortcuts, resizable/rearrangeable persisted layouts, recent working context and save/recovery handling. Verify the two applications exercise those facilities without sample-specific assumptions. |
| M8 behavior authoring, before workflow acceptance | First exercise bounded scripts with per-object properties and atomic error/reload recovery in Motion Lab. Then select language/tooling using real creator evidence and integrate persisted attachments, undo, reference fields, script templates, configured external editing, navigable diagnostics and debugging into the generic editor. The initial Rhai application experiment does not satisfy this row. |
| M9 adoption and remaining corrections | Experienced Unity and Unreal creator walkthroughs, readable scaling/focus/accessibility review and fixes for supported journeys; verify recovery and distribution on the declared support matrix. No readiness claim while these baseline blockers remain. |

M8 will need several runnable implementation slices; estimate each from its actual requirements rather than compressing it into a single example task. Shared changes preserve CLI/editor/MCP semantics where those operations are exposed. Avoid promising screen-reader compliance, crash recovery guarantees or a shortcut preset without corresponding implementation and evidence.


The [post-M8 persistence reassessment](ROADMAP-REVIEW.md#post-m8-persistence-reassessment--2026-11-04) sequences the remaining allocation in the canonical roadmap. That historical construction sequence is implemented, including multi-selection and bounded gizmos; physical focused handle input remains unverified. Reuse, editor authoring and behavior/tooling are reported separately, with all currently allocated M8 gates retained.


2026-11-04 construction update: generic create/display-name/duplicate/delete, new empty scene and persisted lifecycle undo now pass the [native construction journey](evidence/M8-objects-native.md). Earlier absence statements describe the M6 baseline. Subsequent manipulation/content/workspace/recovery evidence is linked in STATUS; the current tooling closeout still does not establish full human familiarity. The user reports mainly Unity experience for a later walkthrough; Unreal-user evidence remains separate.


2026-11-04 manipulation update: ordered multi-selection, window-local copy/paste, grouped commands, transient translation/rotation/scale tools, three pivots, snapping and mixed-component fields are implemented. Real egui and [native controls/property evidence](evidence/M8-manipulation-tools-native.md) distinguish checked behavior from still-unverified physical focused handle input/human familiarity. Exact supported scale semantics exclude shear-producing world nonuniform group scaling; this is explicit UI/guide behavior, not silent approximation. That historical manipulation checkpoint was followed by delivered content/workspace/navigation/remapping/recovery and generic behavior/tooling integration, as linked below.


## First creator feedback — 2026-11-04

The user (mainly Unity experience) tried the verified manipulation build in a disposable workspace2 scene and reported it was good overall. Reported blockers/friction: visibly aliased cubes, Enter did not submit rename (button click required), and missing expected perspective/ISO views. The user explicitly reinforced keyboard fluency: Enter submission, traversal, shortcuts and hotkeys are central quality-of-life requirements. This is one actual creator's qualitative feedback, not completion of the full M9 journey or an Unreal-user study; individual task success/timings were not reported.

The reviewed build was perspective-only and single-sample; the user did not miss an existing projection toggle. Those controls, Enter rename and4× MSAA are now implemented with local quality/cost and integrated native evidence. Physical focused input and creator follow-up remain open. Exact defaults remain agent-selected, as recorded in decisions0015/0017; the broader keyboard-submit/cancel audit continues with new workflows before M8 closure.

2026-11-04 follow-through: workspace/AA/scene5 checkpoint12cde36 passed the [native keyboard/context journey](evidence/M8-workspace-native.md), including2× settings repair. Content search/context/reference drops and bounded recovery passed the [content/recovery journey](evidence/M8-content-recovery-native.md); generic source/properties, unfinished Save and source-draft recovery passed the [behavior journey](evidence/M8-behavior-native.md). External working-copy conflict/Undo/restart promotion passed the [tooling journey](evidence/M8-integrated-tooling.md). Physical focused input and creator follow-up remain distinct.

Remappable Rename (defaultF2) and Select all (defaultCtrl+A outside owned text/modal/gesture) are implemented; real-egui tests protect focus, active identity and remap conflicts. Hidden native injected keys were inconclusive and do not prove physical keyboard behavior. Rotation precision now displays degrees, with exact unchanged axes and internal/CLI/MCP radians preserved. Recovery-v3 unit tags retain invalid v1/v2 radian drafts verbatim with explicit legacy labels; focused migration and full editor tests pass. These frontend defaults are agent-selected responses to confirmed familiarity/keyboard requirements, not user-selected mappings. Debugger breakpoint entry supports owned Enter/Escape and keyboard traversal; final connected native/SDK acceptance passed, including immediate Enter→Escape after the focus repair.


## Revised human walkthrough and visual direction — 2026-11-05

The user immediately found viewport selection unreliable and requested live transform updates in the inspector while dragging. Their supplied inspector screenshot exposed weak hierarchy, unclear controls and reliance on tiny explanatory copy. They require familiar Unity/Godot interaction/copy conventions and a distinct Ruster identity inspired by Obsidian/Notion: clean, simple, readable and aesthetically pleasing. Godot reference use/installation is authorized;4.7.2 was already installed. [EDITOR-DESIGN.md](EDITOR-DESIGN.md) separates these confirmed requirements, primary sources, actual reference observations and agent-selected proposals. Existing M8 injected/native evidence does not overrule this physical user feedback. The user requested autonomous continuation while asleep; preserve their open work.


2026-11-05 repaired-build follow-up: the Unity-experienced user reviewed the latest default release and reports a significant improvement, reliable viewport selection and actively updating Transform values. This is physical confirmation of those two reported repairs. They also confirm closing both review windows. Enter rename, broader keyboard/focus/gizmo paths, final visual approval and the full first-run/recovery/distribution/MCP journey were not individually reported and remain separate.
