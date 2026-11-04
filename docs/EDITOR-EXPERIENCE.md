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
