---
name: ruster-development
description: Guide research, planning, implementation, review, and documentation for the Ruster Rust/Vulkan engine using dynamically selected specialist agents. Apply only to Ruster project work.
---
# Ruster development flow

Read the project's `AGENTS.md` and `docs/STATUS.md` first. Find the project from the working directory or this skill's resolved repository location; do not apply its workflow to unrelated repositories. Respect the current authorization and scope in AGENTS.md and STATUS.md; do not rely on historical phase restrictions after the user advances the project.

The user authorizes selective delegation for this project. Use a balanced coding model as the main developer, a stronger reasoning model for difficult synthesis, an economical model for bounded work, and a visual/computer-use specialist when those capabilities matter. These are roles, not pinned product families or versions.

## Refresh once per root session

Before substantive work, read `docs/workflow/MODEL-ROUTING.md` and refresh its dated snapshot:

1. Inspect the active delegation tool's supported model IDs, reasoning levels, concurrency limits and context inheritance rules. This determines what can actually be spawned. Do not invent IDs from release numbers.
2. Check current official model and subagent guidance linked in that document; use OpenAI Docs when available. Open sources rather than relying on search snippets. Refresh each role independently: releases are staggered. Prefer demonstrated suitability, not automatically the largest version number.
3. Treat local model caches only as timestamped supporting evidence. A cache or public announcement does not establish access through the current tool. Never copy account identity, credentials, or the full cache into the repository.
4. Record check time, evidence links, available choices, role assignments, supported effort, uncertainty, and changes. Prefer subscription/client guidance for this surface; API prices do not establish actual subscription cost. Do not claim savings without measured usage.
5. If official sources are unavailable, label guidance stale/unverified and use the best evidenced model actually exposed by the tool. Continue bounded work; defer only tasks requiring unavailable capabilities. Refresh again after an availability error, an observed change in the host model inventory, or an explicit model-change request, not on every turn. Child agents reuse the root's session snapshot instead of repeating discovery.

This skill does not change the running main model. Recommend the resolved balanced model for the next session when needed; do not silently modify global settings, start replacement sessions, or claim a switch occurred.

## Route by task

- Main developer: owns scope, normal implementation, integration, verification and documentation. Keep small coupled tasks here; no delegation ceremony for trivial work.
- Deep reasoning: ambiguous research synthesis, consequential architecture choices, hard synchronization/ownership defects, or review of costly-to-reverse decisions. Simple fact lookup does not require this tier.
- Economical worker: explicit mechanical edits, bounded extraction, test fixtures, or focused fixes with clear acceptance criteria. Do not assign ambiguous architecture or unsafe-code correctness solely to this role.
- Visual specialist: screenshot interpretation, editor usability, rendering artifact inspection and permitted UI automation. Confirm image/UI tools are available; a visual model cannot create missing tool access. Prefer APIs/CLI for tasks they handle better.

For whole-roadmap reviews and long-horizon product/architecture strategy, the user explicitly prefers a deep-reasoning specialist (currently Astra in the routing snapshot). When the main agent is Sol or another balanced model, delegate the strategic assessment to that role; the main agent gathers implementation evidence, reconciles findings and owns documentation. Do not treat these reviews as routine solo implementation merely because the main model can attempt them. This does not require specialist delegation for a status recap or minor roadmap wording change. If the main agent already fills the deep-reasoning role, it may perform the assessment directly; independent review remains useful when consequential. If that role is unavailable, disclose the limitation and provide a clearly qualified review with available capabilities. Resolve future model versions through the session snapshot rather than pinning them here.

Choose model and effort together using current guidance and task risk. Start at a suitable supported effort; maximum effort is not a default. Reserve escalation for uncertainty, failed validation, or consequential decisions. After one focused repair attempt fails, reassess the approach or escalate rather than repeat cheap attempts indefinitely.

## Delegate with a contract

Give each child: objective, exact relevant paths/context, confirmed requirements versus proposals, permitted actions, exclusive write scope (or read-only), acceptance checks, expected output, and stopping condition. Use minimal context for explicit model overrides; obey the tool's context rules. In the current collaboration harness, explicit overrides require `fork_turns="none"`.

Default to one delegation layer. Children return findings or request escalation rather than spawning further agents unless the root explicitly grants a bounded exception. Use only as much concurrency as independently useful work warrants, within host limits. Never assign concurrent writers to the same files, lockfiles, shared generated outputs, or UI session. Root owns shared status and decision records; readers can work in parallel with disjoint writers. Do not revert another agent's changes.

For research require primary sources, dates, competing alternatives and explicit uncertainty. For code require changed paths, checks/results and unresolved risks. For visual work require observed evidence and reproduction steps; screenshots alone do not prove performance or correctness.

## Integrate and learn

The main agent inspects actual diffs and evidence, resolves disagreements, and follows `skills/ruster-testing/SKILL.md` and `docs/TESTING.md` for appropriate verification before accepting work. Reuse valid final-state worker results; do not automatically rerun them. Specialist confidence is not verification. Use independent review for high-consequence changes; avoid mandatory premium review for every edit.

Maintain current state and decisions according to AGENTS.md. Record meaningful delegation outcomes in `docs/workflow/DELEGATION-LOG.md`: task, role/model/effort, reason, result, repair/escalation, and observed elapsed/usage data if available. Mark unavailable metrics unknown. Assess cost per accepted result, including review and rework; do not optimize solely for cheap calls or number of parallel agents.

Update routing based on repeated relevant outcomes. A new model can be tried on a bounded real task before taking consequential work; do not launch benchmark swarms or billable API jobs merely to refresh the catalog.
