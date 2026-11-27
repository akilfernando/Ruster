# Playbook: how to work on Rot Engine

Adopted 2026-11-17 at the user's request for a lean way of working that any
agent (Claude Code or Codex, strong model or mid-tier) can follow. Revised
2026-11-27, at the user's request, so that cost-efficient models can
implement packages on their own. If another document disagrees with this one
about *how to work*, this one wins.

The user's instruction in one line: **spend effort on features and fixes that
change the product, not on supporting process.**

## 1. Start of a session (10 minutes, no more)

1. `git status`, `git log --oneline -5`. The **trunk** is named under "Where
   to work" in [STATUS](STATUS.md).
2. Read "Now" and "Known problems" in [STATUS](STATUS.md).
3. Open [BACKLOG](BACKLOG.md). Take the first `open` package in your lane,
   unless the user asked for something specific. With no lane given: the
   Repair lane until it is empty, then the UX lane.
4. Read that package's brief in `docs/briefs/`. Read no other brief.
5. Read [ARCHITECTURE](ARCHITECTURE.md) before your first code change. If the
   package touches the editor UI, read [UX-STANDARDS](UX-STANDARDS.md).
6. Check the brief's **Facts** against the code. If one is wrong, correct
   the brief in its own commit, then go on. If the brief's design no longer
   fits the code, stop and follow [section 10](#10-briefs).
7. Claim the package: set its status to `in progress — <model>, <date>` and
   commit that one line on the trunk, so other agents see it.

Do not read `docs/archive/`, `docs/evidence/`, `docs/HISTORY.md`, old plans or
the old roadmap unless your brief links to a specific section. They are
history and cost tens of thousands of tokens.

## 2. The loop for one package

1. **Follow the brief.** Do its steps in order. It names the reference
   behavior, the files and the checks.
2. **Look at what exists.** Run the editor with
   [`scripts/ui_drive.py`](../scripts/ui_drive.py) and view the screenshots.
   Read the code the brief names.
3. **Build a thin end-to-end version first.** Data, runtime, editor control,
   MCP, save and reopen, built game, in that order, as far as the package
   reaches. Get one real example working through all of it before widening.
4. **Use it.** Add the feature to the sample project the brief names and run
   it in the editor and in the built player.
5. **Protect it.** Write the tests the brief lists. Run the gate.
6. **Finish.**
   - Go down the brief's **Done when** list. Every item is met, or the
     package is not done.
   - Delete the brief file. Remove the package's row from its lane and add
     its id to the phase's line under Done in BACKLOG.
   - Update STATUS if what a creator can do, or a known problem, changed.
   - Commit, merge, push. Report in a short paragraph: what a creator can
     now do, how you checked, what you did not check.

A package should fit in one working session. If it will not, do its steps in
order as far as you get, commit, set the status back to `open`, and say in
the commit which step is next.

## 3. Definition of done

A package is done when its brief's **Done when** list is met and all of
these that apply are true. Name what you skipped in the commit message. If
an item applies and cannot be met, the package stays open.

- A creator can do the thing in the editor, in the place and with the name a
  Unity or Unreal user would expect.
- It is reachable by keyboard, and Undo reverses it as one step.
- It survives Save, close, reopen. It works in a built game if it is a
  runtime feature.
- An AI client can do the same thing through MCP and can discover it, unless
  the package is purely visual.
- The active phase's sample project in `examples/` uses it.
- Nothing a creator could do before is gone. If the package removes or
  replaces a surface, the commit lists what was removed and where each
  capability went.
- It stays inside the [performance budgets](PARITY-PLAN.md#5-performance-budgets).
  If you touched a hot path, measure in a release build before and after.
- You looked at the screenshots and it passes the
  [UI review checklist](UX-STANDARDS.md#10-ui-review-checklist).
- The creator guide and `docs/LUA-API.md` say how to use it.
- `python3 scripts/check.py --base <trunk>` passes, and
  `python3 scripts/check.py --gpu` if you changed what is drawn.

## 4. Quality bars, with the mistakes this project has already made

| Bar | What it means | The mistake to avoid |
| --- | --- | --- |
| Match the reference | Unity and Unreal conventions are the spec for names, placement, shortcuts and defaults. | Inventing terms and flows. An Apply button on the Inspector. |
| Whole slices | A feature is engine, editor, MCP, save, build and guide. | Components an AI app cannot discover. Features no guide mentions. |
| General, not bounded | Build for real projects. Limits are large and soft. | 16 KiB scripts, 10-second sounds, 16 Animators, box and sphere colliders only. |
| Nothing lost | A rewrite keeps what worked. | The editor cutover that deleted docking, layouts and the Game tab while STATUS still said done. |
| Checks that can fail | A test proves something only if it fails when the feature breaks. | Pixel checks that pass by `u8` wraparound. A benchmark run with ten times the shipping limits. |
| Show, do not explain | The UI makes state obvious. Detail goes in tooltips and the Console. | Paragraphs of text under panels and in the status bar. |
| Fast | Interactions feel instant; idle costs nothing. | Each package adding per-tick work until the budget is gone. |
| Honest | Say what was verified and how. | Calling injected-input tests a human walkthrough. Marking a phase done on the implementer's word. |

## 5. Stop signs

If you catch yourself doing one of these, stop and do the thing on the right.

| You are about to… | Do this instead |
| --- | --- |
| Start a package that has no brief | Stop. See [section 10](#10-briefs). |
| Mark a package done with a Done-when item unmet | Leave it open and say which item is missing. |
| Write a paragraph in BACKLOG or STATUS | One status phrase. The detail goes in the commit message. |
| Commit code with an empty message body | Write what changed, the commands you ran and their results. |
| Write hash manifests, receipts, audit folders or "evidence" documents | Put the commands you ran and their result in the commit message. |
| Add a new scene or catalog format version or capability flag | Add an optional field with a default. See [A2](PARITY-PLAN.md#4-architecture-decisions). |
| Add a refusal for content you do not support yet | Import what you can, show one warning in the Console, keep going. |
| Add a fixed limit a normal project could hit | Use a generous safety limit. See [A12](PARITY-PLAN.md#4-architecture-decisions). |
| Add helper text to explain a control | Rename or redesign the control. Use a tooltip if detail is still needed. |
| Add a toolbar row, a top-level button or a new panel for one feature | Put it where the reference engines put it. |
| Grow a file that is already over 3,000 lines | Put the new system in its own module. |
| Build a framework or a harness "for later" | Write the direct code for today's feature. Extract on the third use. |
| Leave fixtures in `/tmp`, or a worktree, or a branch | Use `target/tmp/`, delete it when done, merge and delete the branch. |
| Start the next package's work inside this one | Finish and merge yours. Then claim the next. |
| Change how AI apps are trusted as a side effect | Only in a package that says so, with a decision note and a review by the strongest model. |
| Ask the user to choose an implementation detail | Choose what Unity does, note it in the commit, continue. |

## 6. Two workflows, one editor

Every authoring feature serves both of these:

- **By hand.** The creator uses the editor directly.
- **Side by side with an AI app.** The editor is open next to Claude, Codex,
  ChatGPT, Cursor or similar. The creator prompts; the AI edits the open
  project through MCP; the creator tweaks by hand in between. Both see the
  same scene, share one Undo history and never need to "sync".

Rules that keep the second workflow good:

- New editor commands go through the shared command layer so the menu item,
  the shortcut and the MCP tool are the same code path.
- Prefer a few general MCP tools over one tool per feature. A new component
  type must not need a new tool, and must appear in the component catalog
  that `rot_tools` serves.
- Project data lives in readable files that an AI coding tool can also edit
  directly; the editor reloads them.
- Test the AI route with a real client once per package that adds authoring
  surface. Use a throwaway project with a project-level MCP config. Do not
  edit the user's own AI app configuration without asking.

## 7. What to verify, and with what

| Check | Command | When |
| --- | --- | --- |
| Gate: format, Clippy, affected tests, Markdown | `python3 scripts/check.py --base <trunk>` | Before every merge. Without `--base` it checks only uncommitted changes. |
| GPU probes | `python3 scripts/check.py --gpu` | When you change `rot-render`, a shader, or what goes into a draw frame. Needs the Vulkan GPU. |
| One test while iterating | `cargo test -p <crate> <name>` | While the behavior is changing |
| See and drive the editor | `cargo build -p rot-editor --features inspection` then `python3 scripts/ui_drive.py --help` | Every UI change. View the PNGs. |
| Keyboard reach | `python3 scripts/ui_drive.py … tabwalk:60` | Every new panel or dialog |
| Runtime speed | `cargo test --release -p rot-behavior --locked -- --ignored --nocapture bench_10k_script_updates_per_frame bench_1k_active_rigid_bodies` | When you change the tick, Lua, physics or animation |
| Render speed | The benchmark commands in `crates/rot-render/README.md` | When you change the renderer |
| AI transport | `python3 scripts/mcp_smoke.py` | When you change the bridge, approval or wire behavior |
| Real AI client | Claude Code or Codex against a running editor, private config, throwaway project | Once per package that adds authoring surface |

Tests: write the ones the brief lists, and any other that protects behavior
a creator depends on. Prefer one test through the real command path over
many unit tests of internals. [TESTING](TESTING.md) has the detail.

The editor test window opens on Hyprland workspace 2 without taking focus;
`ui_drive.py` does this for you and opens a disposable copy of the scene.
Never send global keyboard or mouse input and never touch the user's
clipboard or their open windows. Timings from the hidden workspace are
throttled and are not performance evidence.

## 8. Documentation you maintain

| File | What you write | Limit |
| --- | --- | --- |
| [BACKLOG](BACKLOG.md) | Your package's status phrase; new findings as one line under Inbox | 14 KB, enforced by the gate |
| [STATUS](STATUS.md) | What works now, known problems, what is not verified | 6 KB, enforced by the gate |
| Creator guide and `docs/LUA-API.md` | How a creator uses the feature | When behavior changed |
| [ARCHITECTURE](ARCHITECTURE.md) | The map and recipes | When you add a crate, a system or a step to a recipe |
| `docs/decisions/` | A decision that changes architecture, a file format or the AI trust model | Under 40 lines |
| Commit message | Subject starting with the package id. Body: what changed, the commands you ran with their results, benchmark numbers, what you did not check | Required for every commit that touches `crates/` |

Nothing else. No per-package plan documents, evidence documents, delegation
logs or model-routing records. Write dates as `2026-11-17`. Mark who
decided: *user* or *agent*. Never state that the user approved something
they did not.

## 9. Who decides what

- **You decide** implementation details inside the brief, following the
  reference engines. Note anything a later agent would otherwise have to
  rediscover.
- **The brief decides** names, API shapes, fields, defaults and what is out
  of scope. Do not change those on your own.
- **Ask the user** before: anything destructive to their files or projects,
  installing system software, editing their own AI app configuration,
  changing brand assets, or reordering the phases of the plan. Git work
  needs no permission: the user asked on 2026-11-17 for fully autonomous
  pushing, pull requests, reviews, merging and branch hygiene.
- **Changing the plan.** Add a line under **Inbox** in BACKLOG with your
  reason. Do not reorder a lane yourself unless a package is blocked.

## 10. Briefs

A brief is the whole instruction for one package: Outcome, Why, Facts, Do,
Done when, Not now, and the Sample line. It is one file in `docs/briefs/`,
under 6 KB, and it is deleted when the package is done.

- **Who writes them.** The strongest available model (Fable in Claude Code,
  Astra in Codex), from the code, after the phase before it is reviewed.
- **A mid-tier model may write a brief** only for a package whose whole
  shape is a recipe in [ARCHITECTURE](ARCHITECTURE.md): one new component,
  one new Lua function, one new command. Copy the structure of an existing
  brief. Anything that adds a system, changes a file format, changes the AI
  trust model or redesigns a surface waits for the strongest model.
- **When no ready brief is left in your lane,** take another lane. When none
  is left at all, stop and tell the user a planning pass is needed.
- **When a brief is wrong.** A wrong fact: fix it and continue. A wrong
  design: stop, set the status to `open`, add an Inbox line saying what does
  not fit, and take the next package.
- **Exit packages** end at `review`. A different session, on the strongest
  model, reruns the checks and sets them done. The implementer never closes
  a phase.

## 11. Models and helpers

Use what your host offers. Do not research or benchmark models.

| Work | Claude Code | Codex |
| --- | --- | --- |
| Briefs, architecture, a hard bug, review of a phase exit or a risky change | Fable | Astra |
| Implementing a package from its brief | Opus or Sonnet | Sol |
| Code motion, renames, fixtures | Haiku | Luna |
| Looking at many screenshots | A visual-capable model | Terra |

Helpers (subagents): use a read-only search agent to map unfamiliar code.
Use at most two implementing helpers at once, on different lanes, each told
its package id and the commit to start from. Review a helper's diff against
the brief's Done-when list before merging it.

**Autonomous runs** (user, 2026-11-22). Repeat the loop in section 2 while
the user's run limit permits. The goal of each round is the next brief; do
not invent slices. A coordinator, when there is one, checks the Done-when
list itself, runs the gate on the merged trunk, and only then starts the
next package.

## 12. Git and the machine

- One trunk, named in STATUS. Branch `work/<package-id>` from it, merge back
  when the package is done, delete the branch, push.
- Commit as you go. Do not rewrite history on the trunk. Do not leave work
  only in a stash or a worktree.
- Temporary projects and fixtures go under `target/tmp/` in the checkout,
  never in `/tmp` (it is memory on this machine) and never beside the
  repository. Delete them when the package is done.
- Do not create a worktree for a measurement. Build the before and after in
  the same checkout, one after the other.
- `python3 scripts/tidy.py` lists leftovers. Run it at the end of a package
  and remove what is yours.

## 13. Known gotchas

- **`ui_drive.py` on the hidden workspace.** The test window never has OS
  focus, so the editor ignores keyboard shortcuts there; test shortcuts
  in-process (see the `workspace.rs` tests). Screenshots taken after
  key-only input can be stale frames. `click:` matches the first control
  with that exact label (use `LABEL#2`); menu items' labels include their
  shortcut text. Pointer drags cannot be sent; use in-process egui tests.
- **egui focus.** A widget sensed without `FOCUSABLE` drops focus, and
  allocating an extra widget shifts auto ids so the focused one loses
  focus next frame; give helper widgets explicit ids (`ui.interact`). A
  focus move made at the end of a frame is reset; replay it at the next
  frame's start (`focus::move_on_next_frame`).
- **Fonts.** Inter and the egui fallbacks lack some symbols (▾ ⋯ ✖ ⓘ).
  Check a glyph with `fontTools` before using it, or paint it.
- **Pixel comparisons.** Convert `u8` channels to `i32` before adding or
  subtracting. A debug build panics on overflow; a release build wraps and
  the assertion passes for the wrong reason.
- **Benchmarks.** Use `Limits::default()`. Close other heavy programs.
- **Flaky tests under load.** `connection/worker.rs`
  `conflict_never_reads_back…` and `bridge.rs` `enrolled_local_transport…`
  failed once each under heavy parallel load; if the gate fails only there,
  rerun once and add an Inbox line if it repeats.
- **Vendored crates.** Read `ROT-PATCH.md` in the crate before upgrading.

## 14. When the user gives feedback

Treat it as the highest-priority input. Fix small things immediately. For
anything larger, add a one-line **Inbox** item in BACKLOG quoting the user's
words, and tell them where it sits in the order.
