# Parity plan and working framework

2026-11-17. Written by the agent (Claude Code, `claude-fable-5-2`) at the
user's request for a parity plan and a lean framework other agents can
follow.

## Context

The user set the goal of feature and UX parity with Unity and Unreal, with
performance as a core requirement, two first-class workflows (by hand, and
beside an AI app over MCP with hands-off setup), an opt-out Rot theme, and
editor UI that follows industry standards. They asked that mid-tier agents
be able to continue the work efficiently without overbuilt supporting
process. The repository then held about 600 KB of process and status
documents, a 265 KB status file, and several rules that required evidence
documents, routing refreshes and delegation logs for each piece of work.

## Decision

- [PLAYBOOK](../PLAYBOOK.md) is the single description of how to work. It
  replaces the earlier process rules in `AGENTS.md` and the two skills,
  which now point to it.
- [PARITY-PLAN](../PARITY-PLAN.md) and [BACKLOG](../BACKLOG.md) replace the
  roadmap as the way to choose work.
- [UX-STANDARDS](../UX-STANDARDS.md) is the editor standard.
- [STATUS](../STATUS.md) is one page; earlier entries moved to
  `docs/archive/`.
- Retired: per-package plan and evidence documents, the delegation log, the
  per-session model-routing refresh, hash manifests and retained-failure
  records.
- The architecture decisions A1–A12 in the plan are agent-selected defaults.
  A10 replaces the session-bound approval model of
  [decision 0050](0050-session-bound-mcp-authority.md) with trust granted
  once per AI app.

## Why

The earlier process produced careful verification of narrow prototype
features while the editor's everyday experience and the engine's limits
stayed far from the goal. Short documents, a single ordered backlog,
reference-engine conventions as the specification, and looking at the real
UI are cheaper and bear directly on the product.

## Cost

Less written evidence per change, and several earlier user-approved process
rules are set aside. The user confirmed requirements are kept in
`AGENTS.md`. The user can restore any rule or veto any architecture
decision; nothing here has been approved by the user item by item.
