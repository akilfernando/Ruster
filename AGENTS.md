# Project continuity

Read `skills/ruster-development/SKILL.md` at the start of each root session and follow its model refresh and selective delegation workflow. This project explicitly authorizes delegation under that skill. Child agents use the root's model snapshot and bounded assignment.

For changes and verification, apply `skills/ruster-testing/SKILL.md` and the single policy in `docs/TESTING.md`. Select affected checks, justify new tests by unique failure modes, and reuse valid final-state evidence rather than rerunning it across agents.

Read `docs/STATUS.md` before project work, then the relevant design and research documents linked there. Inspect actual code, configuration, and git state before making claims about implementation. Documentation is context, not proof of current behavior.

For performance-sensitive architecture and optimization, follow `docs/PERFORMANCE.md`: learn from primary engine sources, record tradeoffs, measure before/after, and revise foundations when evidence warrants it. Do not claim competitive superiority from language choice or toy benchmarks.

The user requires proper documentation throughout the work so future Codex sessions do not infer decisions or rely on stale data.

For each meaningful change, update the relevant documentation in the same work session:

- Keep `docs/STATUS.md` current: implemented state, verification, next steps, unresolved questions, and blockers. Date substantive updates.
- Separate confirmed user requirements, proposed choices, accepted decisions, and verified implementation. Never promote a proposal through repetition or user silence.
- Record consequential accepted architecture decisions in `docs/decisions/`, including context, alternatives, rationale, consequences, status, and superseding decisions.
- Keep research sources and access dates with claims; label conclusions and untested hypotheses. Recheck changeable facts before relying on them.
- Record verification commands and results, including failures and checks not performed. Do not claim tests or benchmarks that were not run.
- Update affected setup/API/workflow documentation when behavior changes. Remove or explicitly supersede obsolete guidance; avoid competing sources of truth.
- Finish substantial work with a concise handoff in `docs/STATUS.md`. Keep current state concise; historical milestones belong in `docs/HISTORY.md`.

The user has authorized engine development as of 2026-11-04. Follow `docs/DIRECTION.md` and decision records for the current working scope. Rust, Vulkan, openness, native MCP, documentation continuity and developer experience are requirements. Distinguish agent-selected defaults from explicit user decisions.

The user-confirmed [project ethos](docs/DIRECTION.md#project-ethos-serve-the-creator) is to make systems serve creators and their intended creations. Evaluate work by the creation workflow it enables, while preserving explicit milestone scope and distinguishing ambition from verified capability.

The user requires immediate familiarity for experienced Unity/Unreal creators and treats everyday editor quality of life as non-negotiable. Follow [EDITOR-EXPERIENCE.md](docs/EDITOR-EXPERIENCE.md) for relevant workflow acceptance; do not defer routine usability as optional polish or infer it from system tests.

The target creator is comfortable using a keyboard. Treat Enter-to-submit, Escape-to-cancel, keyboard traversal, discoverable/remappable shortcuts and focus ownership as required workflow behavior, including dialogs and numeric/name fields. The 2026-11-04 Unity walkthrough also requires usable antialiasing and perspective/orthographic (ISO) plus axis-view navigation; verify actual quality/cost and interaction rather than relying on system tests.

## Local desktop testing

The user works on workspace 1. Open Ruster test windows on Hyprland workspace 2 silently, without switching the active workspace or stealing focus. A local `ruster-development-workspace` window rule in `~/.config/hypr/hyprland.lua` routes initial Ruster viewer/editor titles there. Verify routing before UI automation; target the exact test window and never send global input to the user's active window. Do not change the user's clipboard for unattended tests. Hidden-workspace rendering may be throttled: label performance evidence accordingly and compare matching conditions. Visual interaction that cannot run in isolation must wait for a suitable opportunity rather than interrupt the user.
