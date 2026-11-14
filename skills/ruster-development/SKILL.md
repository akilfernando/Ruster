---
name: ruster-development
description: Guide research, planning, implementation, review, and documentation for the Ruster Rust/Vulkan engine using dynamically selected specialist agents. Apply only to Ruster project work.
---
# Ruster development flow

Read the project's `AGENTS.md` and `docs/STATUS.md` first. Find the project from the working directory or this skill's resolved repository location; do not apply its workflow to unrelated repositories. Respect the current authorization and scope in AGENTS.md and STATUS.md; do not rely on historical phase restrictions after the user advances the project.

The user authorizes selective delegation for this project. Use a balanced coding model as the main developer, a stronger reasoning model for difficult synthesis, an economical model for bounded work, and a visual/computer-use specialist when those capabilities matter. These are roles, not pinned product families or versions.

## Outcome and throughput controls

Before substantial implementation, the root records one creator outcome, actual interaction route, accountable coordinator/writer, short end-to-end exit test, non-goals, required risk checks and progress budget in the existing handoff/status. Run a cheap executable preflight of the complete critical path before expensive runtime snapshots or desktop sessions. Discovery, owner establishment, human approval and successful authoring are separate stages; an alternate interaction channel is not acceptance of the intended one.

Reassess after 30 minutes without that end-to-end smoke or two failed journey attempts; justify a larger initial budget for consequential work. Carry elapsed work and failures across handoffs rather than resetting them for a renamed slice. Diagnose the whole remaining path before another seam-sized assignment. Keep one implementation owner through coupled repairs; main-agent implementation is allowed, and specialist review is consolidated around meaningful risk boundaries. Briefs normally stay near 400 words plus needed interface constraints and evidence links.

Freeze the scope and threat model. Classify new findings as outcome blockers, later production/release gates or nonblocking follow-ups; do not silently promote optional qualification into a new product requirement or weaken existing safeguards. Time-box expanding tooling prerequisites against a finite caller inventory and explicit exit proof. Preflight realistic harness phase timing and cleanup margin: an arbitrary test deadline is not a user-approved product performance limit. Revise harness-only limits transparently when justified, never hide a missed actual performance requirement or retry uncertain mutations.

Use affected checks while iterating, reuse valid unchanged-input evidence, checkpoint/push coherent work, and report completed creator workflows and actual blockers rather than productivity by commit/test counts. Keep the current handoff concise and reference existing evidence; no new scheduler, telemetry system or per-tiny-change paperwork is required.

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

Choose model and effort together using current guidance and task risk. Apply the following user-approved project starting policy from 2026-11-11.

- Balanced developer at low effort (currently Sol6.1) for routine implementation, focused fixes, documentation and status. This is a project choice to evaluate through outcomes; official guidance recommends the client's default effort and task-specific adjustment.
- Balanced developer at medium for multi-system changes or debugging with meaningful uncertainty; high for well-defined problems requiring deeper tracing, ownership analysis or edge-case reasoning.
- Deep-reasoning specialist at low (currently Astra) for consequential architectural judgment, competing approaches or a stalled diagnosis. Raise effort when the problem warrants it. Sol high and Astra low are task-dependent alternatives with unmeasured relative project cost; never assume equal effort labels imply equal capability or expense.
- Economical workers (currently Luna) for bounded mechanical work with explicit acceptance criteria. Use the session snapshot's model-specific starting effort and assess repair overhead before expanding their responsibilities.
- Visual specialists (currently Terra) for concrete image/UI work where their capabilities add value.

Select justified higher effort immediately for known difficult or high-consequence work. After one focused repair attempt fails, reassess the diagnosis, context and approach or escalate rather than repeat cheap attempts indefinitely. Do not route merely to meet model-share quotas. These instructions select child settings; they do not change the running main model or effort.

## Delegate with a contract

Give each child: objective, exact relevant paths/context, confirmed requirements versus proposals, permitted actions, exclusive write scope (or read-only), acceptance checks, expected output, and stopping condition. Use minimal context for explicit model overrides; obey the tool's context rules. In the current collaboration harness, explicit overrides require `fork_turns="none"`.

Keep small coupled tasks in the main agent. Delegate when an independent assignment is substantial enough to justify briefing, review and integration. Prefer a fresh minimal-context assignment when an existing worker's accumulated history is mostly irrelevant; retain a worker when its relevant context avoids rediscovery. Return concise findings, changed paths and evidence references instead of large copied logs.

Default to one delegation layer. Children return findings or request escalation rather than spawning further agents unless the root explicitly grants a bounded exception. Use only as much concurrency as independently useful work warrants, within host limits. Never assign concurrent writers to the same files, lockfiles, shared generated outputs, or UI session. Root owns shared status and decision records; readers can work in parallel with disjoint writers. Do not revert another agent's changes.

For research require primary sources, dates, competing alternatives and explicit uncertainty. For code require changed paths, checks/results and unresolved risks. For visual work require observed evidence and reproduction steps; screenshots alone do not prove performance or correctness.

## Integrate and learn

The main agent inspects actual diffs and evidence, resolves disagreements, and follows `skills/ruster-testing/SKILL.md` and `docs/TESTING.md` for appropriate verification before accepting work. Reuse valid final-state worker results; do not automatically rerun them. Specialist confidence is not verification. Use independent review for high-consequence changes; avoid mandatory premium review for every edit. Consolidate specialist review around a coherent implementation and evidence package. Request follow-up when a repair changes the assessed risk, new evidence creates uncertainty or a material question remains; an unchanged evidence package does not itself justify another specialist pass.

Maintain current state and decisions according to AGENTS.md. Record meaningful delegation outcomes in `docs/workflow/DELEGATION-LOG.md`: task, role/model/effort, reason, result, repair/escalation, and observed elapsed/usage data if available. Mark unavailable metrics unknown. Assess cost per accepted result, including review and rework; do not optimize solely for cheap calls or number of parallel agents.

For the next two bounded development slices after resumption, record model/effort, acceptance or repair count, useful review findings and available elapsed/usage data in the existing delegation log. Evaluate total consumption alongside accepted work and regressions. Aggregate model shares cannot attribute main-agent versus worker consumption or establish savings; credit rates do not directly establish included weekly allowance usage. Keep unavailable metrics unknown and avoid a separate telemetry project.

Update routing based on repeated relevant outcomes. A new model can be tried on a bounded real task before taking consequential work; do not launch benchmark swarms or billable API jobs merely to refresh the catalog.
