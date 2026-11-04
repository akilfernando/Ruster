# 0002 Proportionate verification

Date: 2026-11-04. Status: adopted under the user's explicit request for diligent testing with minimal suite bloat and development overhead.

## Decision

Use one documented risk-based policy, a project testing skill and a small local runner. Select Rust tests by changed workspace packages and transitive reverse dependencies. Documentation-only edits avoid compilation. Shared build inputs, unknown paths and the initial uncommitted repository select full scope. PR verification uses the actual target reference rather than HEAD alone.

Sensitive changes require supplemental evidence described in docs/TESTING.md. Separate GPU, stress, performance and platform work from the universal fast gate. Do not implement speculative suites for subsystems that do not exist.

Reuse valid final-state evidence across agents, add tests for unique observable failure modes, consolidate redundant cases, and track meaningful slow/flaky checks. Timing budgets are investigation triggers rather than permission to omit validation.

## Alternatives and consequences

Always running every suite is simple but scales poorly. Filename-only package selection misses dependent behavior; reverse dependency closure is safer. A sophisticated historical test-impact cache might be faster at scale but introduces stale evidence and maintenance complexity; Cargo incremental caching is sufficient now.

The runner is deliberately conservative and cannot prove semantic impact or substitute for GPU validation. No hosted CI or branch protection is configured by this decision. Before remote PR workflows begin, wire the same policy into CI and define the authoritative merge-result check.

## Revisit

Use observed timings, escaped regressions and test maintenance cost to refine selection. Revisit feature/platform coverage when those become supported contracts. Do not use coverage percentage or test count as the optimization target.
