# Playbook: how to work on Rot Engine

Adopted 2026-11-17 at the user's request for a lean way of working that any
agent (Claude Code or Codex, strong model or mid-tier) can follow. It replaces
the earlier process rules. If another document disagrees with this one about
*how to work*, this one wins.

The user's instruction in one line: **spend effort on features and fixes that
change the product, not on supporting process.**

## 1. Start of a session (5 minutes, no more)

1. `git status`, `git log --oneline -5`. The **trunk** is the branch named
   under "Where to work" in [STATUS](STATUS.md). Run `git worktree list` if
   anything looks odd; this repository has had stale checkouts.
2. Read the current work and known gaps in [STATUS](STATUS.md). Look up
   earlier results when they affect the package.
3. Open [BACKLOG](BACKLOG.md). Take the first open package in your lane
   under **Now**, unless the user asked for something specific. With no lane
   given, use the UX lane. Read only that package, and do only that package.
4. If the package touches the editor UI, read [UX-STANDARDS](UX-STANDARDS.md).
5. Claim the package: change its status line to
   `in progress — <agent/model>, <date>` and commit that one-line change
   directly on the trunk, before you branch, so other agents see it.

Do not read `docs/archive/`, `docs/evidence/`, `docs/HISTORY.md`, old plans or
the old roadmap unless your package links to a specific section. They are
history and cost tens of thousands of tokens.

## 2. The loop for one package

1. **Look at the reference.** Reuse an established convention from the
   [UX standards](UX-STANDARDS.md) or the package brief. For a new or
   disputed interaction, use official Unity and Unreal documentation to
   check placement, names, defaults and shortcuts. The
   [editor comparison](research/EDITOR-WORKFLOW-COMPARISON.md) links
   the main editor pages.
2. **Look at what exists.** Run the editor with
   [`scripts/ui_drive.py`](../scripts/ui_drive.py) and view the screenshots.
   Read the code the package lists under *Touch*.
3. **Build a thin end-to-end version first.** Data → engine → editor control →
   MCP → save and reopen → built game, in that order, as far as the package
   reaches. Get one real example working through all of it before widening.
4. **Use it and look at it.** Run the changed journey in a disposable project
   in one representative appearance. Use a second size for layout or scaling
   risk. Check the specific controls whose appearance changes in the other
   theme. See [testing appearance](TESTING.md#risk-specific-evidence).
5. **Protect it.** Add one to three tests that would catch a real regression.
   Run the gate.
6. **Finish.** Tick the package in BACKLOG, update STATUS, update the user
   guide if behavior changed, commit, merge. Report in a short paragraph:
   what a creator can now do, how you checked, what you did not check.

A package should fit in one working session. If it will not, split it by
user-visible steps and do the first one completely.

### Autonomous runs

For an authorized breadth-first run, have Astra choose a bounded creator
workflow from the current product gaps. Favor a missing outcome that unlocks
a usable editor or game journey. Rotate to another underdeveloped area once
the current one can serve that journey. Use the parity plan's reference
projects to tie slices together.

Give Sol the base commit, creator outcome, acceptance scenario, relevant
risks and the point at which the slice is done. Agree on the performance
question and measurement boundary before building a benchmark. The
coordinator owns the integration gate and reuses the implementer's evidence
where the relevant inputs still match. Share fixtures and builds. Coordinate
Cargo use and keep competing work away from timed measurements.

Use hosted CI waits for independent review or the next read-only brief when
authorized. Use the final hosted result for the integrated revision. Check
any user-set run limit before starting another goal.

## 3. Definition of done

A package is done when all of these that apply to it are true. Skip the
items that cannot apply (a transport change has no screenshots; a visual
change has no MCP tool) and name what you skipped in the handoff. If an item
applies and cannot be met, say so instead of quietly skipping it.

- A creator can do the thing in the editor, in the place and with the name a
  Unity or Unreal user would expect.
- It is reachable by keyboard, and Undo reverses it as one step.
- It survives Save, close, reopen. It works in a built game if it is a
  runtime feature.
- An AI client can do the same thing through MCP (see §6), unless the package
  is purely visual.
- It works on representative content. Include a real asset for asset-facing
  changes and a large scene for features that change how work scales with
  scene size. It warns and continues where it can instead of refusing.
- It stays inside the [performance budgets](PARITY-PLAN.md#5-performance-budgets)
  for its phase. If you touched a hot path, measure in a release build
  before and after and put both numbers in the commit message.
- You looked at the screenshots and it passes the
  [UI review checklist](UX-STANDARDS.md#10-ui-review-checklist).
- `python3 scripts/check.py --base <trunk>` passes.
- BACKLOG and STATUS are updated.

## 4. Quality bars, with the mistakes this project has already made

| Bar | What it means | The mistake to avoid |
| --- | --- | --- |
| Match the reference | Unity and Unreal conventions are the spec for names, placement, shortcuts and defaults. | Inventing terms and flows ("Reset" before Play, "Control runtime", "accepted generation"). |
| Whole slices | A feature is engine + editor + MCP + save + build. | Engine-only features with no usable editor route, or a UI over a one-off demo. |
| General, not bounded | Build for real projects. Limits are large and soft. | "One clip", "one sphere", 64 assets, 256 objects, refusing a model because it has a normal map. |
| Show, do not explain | The UI makes state obvious. Detail goes in tooltips and the Console. | Paragraphs of contract text under panels and in the status bar. |
| Fast | Interactions feel instant; idle costs nothing. | Shipping a feature without ever running it in release on a large scene. |
| Honest | Say what was verified and how. | Calling injected-input tests a human walkthrough. |

## 5. Stop signs

If you catch yourself doing one of these, stop and do the thing on the right.

| You are about to… | Do this instead |
| --- | --- |
| Write hash manifests, receipts, audit folders or "evidence" documents | Put the commands you ran and their result in the commit message. |
| Record failed attempts and harness retries in documentation | Fix it and move on. Mention only failures the next person must know about. |
| Add a new scene or catalog format version, capability flag or "kit profile" for a feature | Add an optional field with a default. See the [format policy](PARITY-PLAN.md#4-architecture-decisions). |
| Add a refusal for content you do not support yet | Import what you can, show one warning in the Console, keep going. Refuse only corrupt or unsafe input. |
| Add helper text to explain a control | Rename or redesign the control. Use a tooltip if detail is still needed. |
| Add a toolbar row, a top-level button or a new panel for one feature | Put it where the reference engines put it: a component in the Inspector, a context menu, an existing menu. |
| Spend more than a day with nothing visible to show | Cut scope to the smallest visible step and ship that. |
| Ask another agent to review routine work | Review your own screenshots and diff. Ask for review only for data-loss risk, the AI trust boundary or a format change. |
| Build a framework, a generic abstraction or a harness "for later" | Write the direct code for today's feature. Extract on the third use. |
| Keep a long-lived branch or a new worktree | Merge when the package is done. Delete the branch. |
| Start the next package's work inside this one | Finish and merge yours. Then claim the next. |
| Change how AI apps are trusted, or weaken an approval step, as a side effect | Only in a package that says so, with a decision note and a review by the strongest model. |
| Write a long STATUS entry | Eight lines. What works now, what is still missing. |
| Ask the user to choose an implementation detail | Choose what Unity does, note it in the package, continue. |

## 6. Two workflows, one editor

Every authoring feature serves both of these:

- **By hand.** The creator uses the editor directly.
- **Side by side with an AI app.** The editor is open next to Claude, Codex,
  ChatGPT, Cursor or similar. The creator prompts; the AI edits the open
  project through MCP; the creator tweaks by hand in between. Both see the
  same scene, share one Undo history and never need to "sync".

Rules that keep the second workflow good:

- New editor commands go through the shared command layer so the menu item,
  the shortcut and the MCP tool are the same code path. Do not write a
  parallel MCP-only implementation.
- Prefer a few general MCP tools over one tool per feature. A new component
  type must not need a new tool.
- Project data lives in readable files (scenes, prefabs, materials, scripts)
  that an AI coding tool can also edit directly; the editor reloads them.
- Test the AI route with a real client at least once per package that adds
  authoring surface, not only through unit tests. Use a throwaway project
  with a project-level MCP config for this. Do not edit the user's own AI
  app configuration without asking.

## 7. What to verify, and with what

| Check | Command | When |
| --- | --- | --- |
| Gate: format, Clippy, affected tests, Markdown links | `python3 scripts/check.py --base <trunk>` | Before every merge. Without `--base` it only checks uncommitted changes, so after committing it checks nothing. |
| Focused regression while iterating | `cargo test -p <crate> <name>` | When the affected behavior changes |
| See and drive the editor UI | `cargo build -p rot-editor --features inspection` then `python3 scripts/ui_drive.py --help` | Every UI change |
| Keyboard reach | `python3 scripts/ui_drive.py … tabwalk:60` and read the order; add `shot:` steps to see the focus ring | Every new panel or dialog |
| Speed | Measure the changed path in release. Use `scripts/measure_budgets.py` for its relevant workloads and observe the desktop isolation rules | When changing a hot path or making a performance claim |
| AI transport | `python3 scripts/mcp_smoke.py` or a focused equivalent | When changing the bridge, approval, routing or wire behavior. Reuse a real-client journey that covers the same contract |
| Real AI client | Claude Code or Codex against a running editor with a private config in a throwaway project | Once per package that adds an authoring surface. Check discovery and the changed action, including shared Undo where applicable |

Tests: write the few that protect behavior a creator depends on. Prefer one
test through the real command path over many unit tests of internals. Do not
add tests to demonstrate diligence. [TESTING](TESTING.md) has the detail.

The editor test window opens on Hyprland workspace 2 without taking focus;
`ui_drive.py` does this for you, and it opens a disposable copy of the scene
you give it (for example `--scene examples/arrangement.scene.json`). Never
send global keyboard or mouse input and never touch the user's clipboard or
their open windows.

## 8. Documentation you maintain

| File | What you write | Limit |
| --- | --- | --- |
| [BACKLOG](BACKLOG.md) | Status line of your package; new findings as one-line items under **Inbox** | One line each |
| [STATUS](STATUS.md) | What works now, what is in progress, known problems | The whole file stays under 100 lines; move old entries out |
| User guides (`docs/CREATOR-WORKFLOW.md`, crate READMEs) | How a creator uses the feature | Only when behavior changed |
| `docs/decisions/` | A decision that changes architecture, a file format or the AI trust model | Under 40 lines: context, decision, why, what it costs |
| Commit message | What changed, how you verified it | A few lines |

Nothing else is required. No per-package plan documents, evidence documents,
delegation logs or model-routing records. Write dates as `2026-11-17`. Mark
who decided: *user* or *agent*. Never state that the user approved something
they did not.

## 9. Who decides what

- **You decide** implementation, API shapes, UI details inside the standards,
  defaults and limits. Follow the reference engines. Note anything a later
  agent would otherwise have to rediscover.
- **Ask the user** before: anything destructive to their files or projects,
  installing system software, editing their own AI app configuration,
  changing brand assets, or reordering the phases of the plan. Git work
  needs no permission: the user asked on 2026-11-17 for fully autonomous
  pushing, pull requests, reviews, merging and branch hygiene.
- **Changing the plan.** Add a line under **Inbox** in BACKLOG with your
  reason. Do not reorder **Now** yourself unless a package is blocked.

## 10. Models and helpers

Use what your host offers. Do not research or benchmark models.

| Work | Claude Code | Codex |
| --- | --- | --- |
| Writing or revising a package brief, architecture, a hard bug, review of a risky change | Fable | Astra |
| Implementing a package | Opus | Sol |
| Mechanical edits, renames, fixtures | Sonnet or Haiku | Luna |
| Judging screenshots | The working model reads the images | Terra, or the working model |

Packages marked **brief needed** must get their brief from the strongest model
available before anyone implements them. Implementers follow the brief; if it
is wrong, fix the brief first.

Helpers (subagents): use a read-only search agent to map unfamiliar code. Use
at most two implementing helpers at once, on files that do not overlap, each
in its own worktree, each told the exact commit to start from, its files, and
its done-when list. Otherwise work in the main session.
Helpers that cannot write to the main checkout (sandboxed worktrees) push
their branch instead; the coordinating session merges it.

## 11. Git

- One trunk, named in STATUS. Branch `work/<package-id>-<short-name>` from
  it, merge back into it locally when the package is done, delete the
  branch. Merging into the trunk needs no permission; pushing follows the
  policy in STATUS.
- Commit as you go. One package may be several commits.
- Do not rewrite history on the trunk. Do not leave work only in a stash or a
  worktree.
- The push policy is the user's and is recorded in STATUS.
- Tool-created worktrees (`.claude/worktrees/`, `.codex-attachments/`) are
  local leftovers. Once their branch is merged or pushed, remove them with
  `git worktree remove --force <path>`.
- After acceptance, retire redundant binaries and delivery copies created
  for the slice. Keep the fixture, useful logs and any artifact needed for a
  reproducible result or an open failure.

## 11a. Known gotchas (found in Phase 1)

- **`ui_drive.py` on the hidden workspace.** The test window never has OS
  focus, so the editor ignores keyboard shortcuts there; test shortcuts
  in-process (see `workspace.rs` tests). Screenshots taken after key-only
  input can be stale frames; use `--foreground` only when the user is away,
  or check focus in-process. `click:` matches the first control with that
  exact label (use `LABEL#2`); menu items' labels include their shortcut
  text (`"Command Palette… Ctrl+K"`). Pointer drags cannot be sent; use
  in-process egui event tests.
- **egui focus.** A widget sensed without `FOCUSABLE` drops focus, and
  allocating an extra widget shifts auto ids so the focused one loses
  focus next frame; give helper widgets explicit ids (`ui.interact`). A
  focus move made at the end of a frame is reset; replay it at the next
  frame's start (`focus::move_on_next_frame`).
- **Fonts.** Inter and the egui fallbacks lack some symbols (▾ ⋯ ✖ ⓘ).
  Check a glyph with `fontTools` before using it, or paint it.
- **Flaky tests under load.** `connection/worker.rs`
  `conflict_never_reads_back…` and `bridge.rs`
  `enrolled_local_transport…` failed once each under heavy parallel load;
  if the gate fails only there, rerun once and add an Inbox line if it
  repeats.
- **Vendored crates.** `third_party/egui_tiles` and
  `third_party/transform-gizmo*` carry small patches; read their
  `ROT-PATCH.md` before upgrading.

## 12. When the user gives feedback

Treat it as the highest-priority input. Fix small things immediately. For
anything larger, add a one-line **Inbox** item in BACKLOG quoting the user's
words, and tell them where it sits in the order.
