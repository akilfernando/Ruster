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

Exact shortcuts, navigation presets, layout technology and persistence/recovery mechanisms are not yet selected. Unity and Unreal users may bring different habits: inspect current primary documentation and observe representative users before choosing defaults; use configurable controls or compatibility presets where needed. Familiarity must be demonstrated in interaction, not inferred from a similar-looking screenshot. This requirement does not itself promise project-file compatibility, asset-format parity or the complete feature catalog of either engine.

## Delivery and acceptance

- Include the relevant everyday interactions in each workflow's definition of done. A transform feature includes selection, manipulation, precision, cancellation, focus and undo behavior; an asset feature includes finding, assigning and recovering content.
- Keep a visible gap list as features are introduced. Internal prototype slices can remain bounded, but missing baseline behavior must be identified as unfinished product work rather than silently waived because a lower-level system passes tests.
- Before claiming the editor is ready for the intended audience, require observed walkthroughs from experienced Unity users and experienced Unreal users. Ask them to create a small scene, find/import/assign content, manipulate and duplicate objects, change properties, undo mistakes, run/stop, save/reopen and recover an error using the product's own help. Record where familiar actions fail, where coaching is needed and where work is lost or interrupted.
- Fix blockers in those supported workflows before editor readiness/public-alpha acceptance. Agree concrete task thresholds from the first baseline; no arbitrary usability score or timing guarantee is asserted now. Automated native interaction protects behavior, but cannot establish that people feel at home.
- Apply the existing [testing policy](TESTING.md): focused behavioral checks and a few real journeys, with human evidence for familiarity. Do not duplicate every action at every test layer.

## Current baseline and gaps

At M6, the inspected generic scene editor provides a hierarchy, viewport selection, transform text fields, mesh assignment, bounded scene undo/redo, save/open, dirty/draft guards, orbit/zoom and Fit. Existing native and MCP verification proves the documented narrow journeys. External human familiarity remains untested.

The generic scene editor still lacks the ordinary create/duplicate/delete object workflow and the complete manipulation, discovery and workspace experience described above. The current text transform flow requires Apply/Revert; object selection can be blocked by a pending draft. Undo buttons still say “Undo transform”/“Redo transform” although history includes mesh assignments. Asset catalog operations persist separately from scene undo. These are concrete design/review inputs, not accepted permanent friction or evidence that the baseline is satisfied.

Implementation evidence inspected: `crates/ruster-editor/src/scene_editor.rs`, its README, the scene Session/Preview API and the post-M6 review. This record adds requirements and acceptance guidance only; no UI behavior changed or new user study was conducted.

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


The [post-M8 persistence reassessment](ROADMAP-REVIEW.md#post-m8-persistence-reassessment--2026-11-04) sequences the remaining allocation in the canonical roadmap. Generic object construction is next; a bounded first lifecycle slice does not complete multi-selection/gizmos or the rest of this baseline. Reuse, editor authoring and behavior/tooling are reported separately, with all currently allocated M8 gates retained.


2026-11-04 construction update: generic create/display-name/duplicate/delete, new empty scene and persisted lifecycle undo now pass the [native construction journey](evidence/M8-objects-native.md). Earlier absence statements describe the M6 baseline. The remaining manipulation/content/workspace/tooling allocation is still open; this does not establish familiarity. The user reports mainly Unity experience for a later walkthrough; Unreal-user evidence remains separate.


2026-11-04 manipulation update: ordered multi-selection, window-local copy/paste, grouped commands, transient translation/rotation/scale tools, three pivots, snapping and mixed-component fields are implemented. Real egui and [native controls/property evidence](evidence/M8-manipulation-tools-native.md) distinguish checked behavior from still-unverified physical focused handle input/human familiarity. Exact supported scale semantics exclude shear-producing world nonuniform group scaling; this is explicit UI/guide behavior, not silent approximation. Content/workspace/navigation/remapping/recovery and generic behavior/tooling integration remain open.
